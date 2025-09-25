//! Task management implementation
//!
//! Everything about task management, like starting and switching tasks is
//! implemented here.
//!
//! A single global instance of [`TaskManager`] called `TASK_MANAGER` controls
//! all the tasks in the operating system.
//!
//! Be careful when you see `__switch` ASM function in `switch.S`. Control flow around this function
//! might not be what you expect.

mod context;
mod switch;
#[allow(clippy::module_inception)]
mod task;

use crate::loader::{get_app_data, get_num_app};
use crate::mm::{MapPermission, VirtAddr, VirtPageNum, PageTableEntry};
use crate::sync::UPSafeCell;
use alloc::vec::Vec;
use lazy_static::*;
use switch::__switch;
pub use task::{TaskControlBlock, TaskStatus};

pub use context::TaskContext;

/// The task manager, where all the tasks are managed.
///
/// Functions implemented on `TaskManager` deals with all task state transitions
/// and task context switching. For convenience, you can find wrappers around it
/// in the module level.
///
/// Most of `TaskManager` are hidden behind the field `inner`, to defer
/// borrowing checks to runtime. You can see examples on how to use `inner` in
/// existing functions on `TaskManager`.
pub struct TaskManager {
    /// total number of tasks
    num_app: usize,
    /// use inner value to get mutable access
    inner: UPSafeCell<TaskManagerInner>,
}

/// Inner of Task Manager
pub struct TaskManagerInner {
    /// task list
    tasks: Vec<TaskControlBlock>,
    /// id of current `Running` task
    current_task: usize,
}

lazy_static! {
    /// Global variable: TASK_MANAGER
    pub static ref TASK_MANAGER: TaskManager = {
        let num_app = get_num_app();
        let mut tasks: Vec<TaskControlBlock> = Vec::new();
        for i in 0..num_app {
            tasks.push(TaskControlBlock::new(
                get_app_data(i),
                i,
            ));
        }
        TaskManager {
            num_app,
            inner: unsafe {
                UPSafeCell::new(TaskManagerInner {
                    tasks,
                    current_task: 0,
                })
            },
        }
    };
}

impl TaskManager {
    /// Run the first task in task list.
    fn run_first_task(&self) -> ! {
        let mut inner = self.inner.exclusive_access();
        let next_task = &mut inner.tasks[0];
        next_task.task_status = TaskStatus::Running;
        let next_trap_cx = next_task.get_trap_cx();
        next_trap_cx.kernel_sp = crate::mm::kernel_stack_position(0).1;
        let next_task_cx_ptr = &next_task.task_cx as *const TaskContext;
        drop(inner);
        let mut _unused = TaskContext::zero_init();
        // before this, we should drop local variables that must be dropped manually
        unsafe {
            __switch(&mut _unused as *mut TaskContext, next_task_cx_ptr);
        }
        panic!("unreachable in run_first_task!");
    }

    /// Change the status of current `Running` task into `Ready`.
    fn mark_current_suspended(&self) {
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].task_status = TaskStatus::Ready;
    }

    /// Change the status of current `Running` task into `Exited`.
    fn mark_current_exited(&self) {
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].task_status = TaskStatus::Exited;
    }

    /// Find next task to run and return task id.
    ///
    /// In this case, we only return the first `Ready` task in task list.
    fn find_next_task(&self) -> Option<usize> {
        let inner = self.inner.exclusive_access();
        let current = inner.current_task;
        (current + 1..current + self.num_app + 1)
            .map(|id| id % self.num_app)
            .find(|id| inner.tasks[*id].task_status == TaskStatus::Ready)
    }

    /// Switch current `Running` task to the task we have found,
    /// or there is no `Ready` task and we can exit with all applications completed
    fn run_next_task(&self) {
        if let Some(next) = self.find_next_task() {
            let mut inner = self.inner.exclusive_access();
            let current = inner.current_task;
            inner.tasks[next].task_status = TaskStatus::Running;
            inner.current_task = next;
            let current_task_cx_ptr = &mut inner.tasks[current].task_cx as *mut TaskContext;
            let next_task_cx_ptr = &inner.tasks[next].task_cx as *const TaskContext;
            drop(inner);
            // before this, we should drop local variables that must be dropped manually
            unsafe {
                __switch(current_task_cx_ptr, next_task_cx_ptr);
            }
            // go back to user mode
        } else {
            panic!("All applications completed!");
        }
    }

    /// get the current user token
    fn get_current_token(&self) -> usize {
        let inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].get_user_token()
    }

    /// get the current trap context
    fn get_current_trap_cx(&self) -> &'static mut crate::trap::TrapContext {
        let inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].get_trap_cx()
    }

    /// Get the syscall times of current task
    fn get_current_syscall_times(&self, syscall_id: usize) -> u32 {
        let inner = self.inner.exclusive_access();
        let current = inner.current_task;
        if syscall_id < crate::config::MAX_SYSCALL_NUM {
            inner.tasks[current].syscall_times[syscall_id]
        } else {
            0
        }
    }

    /// Increase the syscall times of current task
    fn increase_syscall_times(&self, syscall_id: usize) {
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;
        if syscall_id < crate::config::MAX_SYSCALL_NUM {
            inner.tasks[current].syscall_times[syscall_id] += 1;
        }
    }

    /// change program brk of current task
    fn change_current_program_brk(&self, size: i32) -> Option<usize> {
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].change_program_brk(size)
    }
    
    /// Get current task's memory set token
    fn current_memory_set_token(&self) -> usize {
        let inner = self.inner.exclusive_access();
        inner.tasks[inner.current_task].memory_set.token()
    }
    
    /// Translate virtual page number for current task
    fn current_memory_set_translate(&self, vpn: VirtPageNum) -> Option<PageTableEntry> {
        let inner = self.inner.exclusive_access();
        inner.tasks[inner.current_task].memory_set.translate(vpn)
    }
    
    /// Insert framed area to current task's memory set
    fn current_memory_set_insert_framed_area(&self, start_va: VirtAddr, end_va: VirtAddr, permission: MapPermission) {
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].memory_set.insert_framed_area(start_va, end_va, permission);
    }
    
    /// Unmap memory range for current task
    fn current_memory_set_munmap(&self, start_vpn: VirtPageNum, end_vpn: VirtPageNum) -> Result<(), ()> {
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].memory_set.munmap(start_vpn, end_vpn)
    }
}

/// Run the first task in task list.
pub fn run_first_task() {
    TASK_MANAGER.run_first_task();
}

/// Switch current `Running` task to the task we have found,
/// or there is no `Ready` task and we can exit with all applications completed
fn run_next_task() {
    TASK_MANAGER.run_next_task();
}

/// Change the status of current `Running` task into `Ready`.
fn mark_current_suspended() {
    TASK_MANAGER.mark_current_suspended();
}

/// Change the status of current `Running` task into `Exited`.
fn mark_current_exited() {
    TASK_MANAGER.mark_current_exited();
}

/// Suspend the current 'Running' task and run the next task in task list.
pub fn suspend_current_and_run_next() {
    mark_current_suspended();
    run_next_task();
}

/// Exit the current 'Running' task and run the next task in task list.
pub fn exit_current_and_run_next() {
    mark_current_exited();
    run_next_task();
}

/// Get the current user token
pub fn current_user_token() -> usize {
    TASK_MANAGER.get_current_token()
}

/// Get the current trap context
pub fn current_trap_cx() -> &'static mut crate::trap::TrapContext {
    TASK_MANAGER.get_current_trap_cx()
}

/// Change program brk of current task
pub fn change_program_brk(size: i32) -> Option<usize> {
    TASK_MANAGER.change_current_program_brk(size)
}

/// Get the syscall times of current task
pub fn get_current_syscall_times(syscall_id: usize) -> u32 {
    TASK_MANAGER.get_current_syscall_times(syscall_id)
}

/// Increase the syscall times of current task
pub fn increase_syscall_times(syscall_id: usize) {
    TASK_MANAGER.increase_syscall_times(syscall_id);
}

/// Get current task's memory set token  
pub fn current_memory_set_token() -> usize {
    TASK_MANAGER.current_memory_set_token()
}

/// Translate virtual page number for current task
pub fn current_memory_set_translate(vpn: VirtPageNum) -> Option<PageTableEntry> {
    TASK_MANAGER.current_memory_set_translate(vpn)
}

/// Insert framed area to current task's memory set
pub fn current_memory_set_insert_framed_area(start_va: VirtAddr, end_va: VirtAddr, permission: MapPermission) {
    TASK_MANAGER.current_memory_set_insert_framed_area(start_va, end_va, permission);
}

/// Unmap memory range for current task
pub fn current_memory_set_munmap(start_vpn: VirtPageNum, end_vpn: VirtPageNum) -> Result<(), ()> {
    TASK_MANAGER.current_memory_set_munmap(start_vpn, end_vpn)
}
