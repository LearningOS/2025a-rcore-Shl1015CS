//!Implementation of [`TaskManager`]
use super::TaskControlBlock;
use crate::sync::UPSafeCell;
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use lazy_static::*;

/// Big stride constant for stride scheduling
const BIG_STRIDE: u8 = 255;

///A array of `TaskControlBlock` that is thread-safe
pub struct TaskManager {
    ready_queue: VecDeque<Arc<TaskControlBlock>>,
}

/// A stride scheduler.
impl TaskManager {
    ///Creat an empty TaskManager
    pub fn new() -> Self {
        Self {
            ready_queue: VecDeque::new(),
        }
    }
    /// Add process back to ready queue
    pub fn add(&mut self, task: Arc<TaskControlBlock>) {
        // Simply add to queue - stride is updated when process is selected
        self.ready_queue.push_back(task);
    }
    
    /// Take a process out of the ready queue using stride scheduling
    pub fn fetch(&mut self) -> Option<Arc<TaskControlBlock>> {
        if self.ready_queue.is_empty() {
            return None;
        }
        
        let mut index = 0;
        let mut min_stride = self.ready_queue[0].inner_exclusive_access().stride;
        
        // Find task with minimum stride using wrapping arithmetic
        for i in 1..self.ready_queue.len() {
            let stride = self.ready_queue[i].inner_exclusive_access().stride;
            // Use signed comparison to handle wrapping
            if ((stride.wrapping_sub(min_stride)) as i8) < 0 {
                index = i;
                min_stride = stride;
            }
        }
        
        // Remove the task with minimum stride
        let task = self.ready_queue.remove(index).unwrap();
        
        // Update the selected task's stride (add pass = BIG_STRIDE / priority)
        {
            let mut inner = task.inner_exclusive_access();
            let pass = BIG_STRIDE / inner.priority;
            inner.stride = inner.stride.wrapping_add(pass);
        }
        
        Some(task)
    }
}

lazy_static! {
    /// TASK_MANAGER instance through lazy_static!
    pub static ref TASK_MANAGER: UPSafeCell<TaskManager> =
        unsafe { UPSafeCell::new(TaskManager::new()) };
}

/// Add process to ready queue
pub fn add_task(task: Arc<TaskControlBlock>) {
    //trace!("kernel: TaskManager::add_task");
    TASK_MANAGER.exclusive_access().add(task);
}

/// Take a process out of the ready queue
pub fn fetch_task() -> Option<Arc<TaskControlBlock>> {
    //trace!("kernel: TaskManager::fetch_task");
    TASK_MANAGER.exclusive_access().fetch()
}
