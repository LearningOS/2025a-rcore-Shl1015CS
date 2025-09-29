//!Implementation of [`TaskManager`]
use super::TaskControlBlock;
use crate::sync::UPSafeCell;
use alloc::collections::BinaryHeap;
use alloc::sync::Arc;
use lazy_static::*;
use core::cmp::Ordering;

/// Wrapper for TaskControlBlock to implement stride scheduling ordering
#[derive(Clone)]
struct SchedulingTask(Arc<TaskControlBlock>);

impl PartialEq for SchedulingTask {
    fn eq(&self, other: &Self) -> bool {
        self.0.get_stride() == other.0.get_stride()
    }
}

impl Eq for SchedulingTask {}

impl PartialOrd for SchedulingTask {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SchedulingTask {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse ordering for min-heap (smallest stride first)
        other.0.get_stride().cmp(&self.0.get_stride())
    }
}

///A array of `TaskControlBlock` that is thread-safe
pub struct TaskManager {
    ready_queue: BinaryHeap<SchedulingTask>,
}

/// High-performance stride scheduler using binary heap.
impl TaskManager {
    ///Create an empty TaskManager
    pub fn new() -> Self {
        Self {
            ready_queue: BinaryHeap::new(),
        }
    }
    /// Add process back to ready queue - O(log n)
    pub fn add(&mut self, task: Arc<TaskControlBlock>) {
        self.ready_queue.push(SchedulingTask(task));
    }
    /// Take a process out of the ready queue - O(log n)
    pub fn fetch(&mut self) -> Option<Arc<TaskControlBlock>> {
        self.ready_queue.pop().map(|scheduling_task| scheduling_task.0)
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
