use crate::sync::{Condvar, Mutex, MutexBlocking, MutexSpin, Semaphore};
use crate::task::{block_current_and_run_next, current_process, current_task, TaskStatus};
use crate::timer::{add_timer, get_time_ms};
use alloc::sync::Arc;

/// Helper function to get current task id for tracing
fn get_current_tid() -> usize {
    current_task()
        .unwrap()
        .inner_exclusive_access()
        .res
        .as_ref()
        .unwrap()
        .tid
}

/// Helper function to get current process id for tracing  
fn get_current_pid() -> usize {
    current_task().unwrap().process.upgrade().unwrap().getpid()
}
/// sleep syscall
pub fn sys_sleep(ms: usize) -> isize {
    trace!("kernel:pid[{}] tid[{}] sys_sleep", get_current_pid(), get_current_tid());
    let expire_ms = get_time_ms() + ms;
    let task = current_task().unwrap();
    add_timer(expire_ms, task);
    block_current_and_run_next();
    0
}
/// mutex create syscall
pub fn sys_mutex_create(blocking: bool) -> isize {
    trace!("kernel:pid[{}] tid[{}] sys_mutex_create", get_current_pid(), get_current_tid());
    let process = current_process();
    let mutex: Option<Arc<dyn Mutex>> = if !blocking {
        Some(Arc::new(MutexSpin::new()))
    } else {
        Some(Arc::new(MutexBlocking::new()))
    };
    let mut process_inner = process.inner_exclusive_access();
    let mid = if let Some(id) = process_inner
        .mutex_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.mutex_list[id] = mutex;
        id as isize
    } else {
        process_inner.mutex_list.push(mutex);
        process_inner.mutex_list.len() as isize - 1
    };
    if process_inner.en_deadlock_detect {
        process_inner.available_mutex.insert(mid as usize, 1);
    }
    mid
}
/// mutex lock syscall
pub fn sys_mutex_lock(mutex_id: usize) -> isize {
    trace!("kernel:pid[{}] tid[{}] sys_mutex_lock", get_current_pid(), get_current_tid());
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    
    // Check bounds
    if mutex_id >= process_inner.mutex_list.len() {
        return -1;
    }
    
    let mutex = match process_inner.mutex_list[mutex_id].as_ref() {
        Some(mutex) => Arc::clone(mutex),
        None => return -1,
    };
    
    // Deadlock detection
    if process_inner.en_deadlock_detect {
        if let Some(&available) = process_inner.available_mutex.get(&mutex_id) {
            if available == 0 {
                return -0xDEAD;
            }
            process_inner.available_mutex.insert(mutex_id, available - 1);
        }
    }
    
    drop(process_inner);
    mutex.lock();
    0
}
/// mutex unlock syscall
pub fn sys_mutex_unlock(mutex_id: usize) -> isize {
    trace!("kernel:pid[{}] tid[{}] sys_mutex_unlock", get_current_pid(), get_current_tid());
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    
    // Check bounds
    if mutex_id >= process_inner.mutex_list.len() {
        return -1;
    }
    
    let mutex = match process_inner.mutex_list[mutex_id].as_ref() {
        Some(mutex) => Arc::clone(mutex),
        None => return -1,
    };
    
    // Update available count for deadlock detection
    if process_inner.en_deadlock_detect {
        if let Some(&available) = process_inner.available_mutex.get(&mutex_id) {
            process_inner.available_mutex.insert(mutex_id, available + 1);
        }
    }
    
    drop(process_inner);
    mutex.unlock();
    0
}
/// semaphore create syscall
pub fn sys_semaphore_create(res_count: usize) -> isize {
    trace!("kernel:pid[{}] tid[{}] sys_semaphore_create", get_current_pid(), get_current_tid());
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let id = if let Some(id) = process_inner
        .semaphore_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.semaphore_list[id] = Some(Arc::new(Semaphore::new(res_count)));
        id
    } else {
        process_inner
            .semaphore_list
            .push(Some(Arc::new(Semaphore::new(res_count))));
        process_inner.semaphore_list.len() - 1
    };
    if process_inner.en_deadlock_detect {
        process_inner
            .available_semaphore
            .insert(id as usize, res_count);
    }
    id as isize
}
/// semaphore up syscall
pub fn sys_semaphore_up(sem_id: usize) -> isize {
    trace!("kernel:pid[{}] tid[{}] sys_semaphore_up", get_current_pid(), get_current_tid());
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    
    // Check bounds
    if sem_id >= process_inner.semaphore_list.len() {
        return -1;
    }
    
    let sem = match process_inner.semaphore_list[sem_id].as_ref() {
        Some(sem) => Arc::clone(sem),
        None => return -1,
    };
    
    // Update available count for deadlock detection
    if process_inner.en_deadlock_detect {
        if let Some(&available) = process_inner.available_semaphore.get(&sem_id) {
            process_inner.available_semaphore.insert(sem_id, available + 1);
        }
    }
    
    drop(process_inner);
    sem.up();
    0
}
/// semaphore down syscall
pub fn sys_semaphore_down(sem_id: usize) -> isize {
    trace!("kernel:pid[{}] tid[{}] sys_semaphore_down", get_current_pid(), get_current_tid());
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    
    // Check bounds
    if sem_id >= process_inner.semaphore_list.len() {
        return -1;
    }
    
    let sem = match process_inner.semaphore_list[sem_id].as_ref() {
        Some(sem) => Arc::clone(sem),
        None => return -1,
    };
    
    // Deadlock detection
    if process_inner.en_deadlock_detect {
        if let Some(&available) = process_inner.available_semaphore.get(&sem_id) {
            if available == 0 {
                let ready_tasks = process_inner
                    .tasks
                    .iter()
                    .filter_map(|task| task.as_ref())
                    .filter(|t| t.inner_exclusive_access().task_status == TaskStatus::Ready)
                    .count();
                if ready_tasks <= 1 {
                    return -0xDEAD;
                }
            }
            process_inner.available_semaphore.insert(sem_id, available.saturating_sub(1));
        }
    }
    
    drop(process_inner);
    sem.down();
    0
}
/// condvar create syscall
pub fn sys_condvar_create() -> isize {
    trace!("kernel:pid[{}] tid[{}] sys_condvar_create", get_current_pid(), get_current_tid());
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let id = if let Some(id) = process_inner
        .condvar_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.condvar_list[id] = Some(Arc::new(Condvar::new()));
        id
    } else {
        process_inner
            .condvar_list
            .push(Some(Arc::new(Condvar::new())));
        process_inner.condvar_list.len() - 1
    };
    id as isize
}
/// condvar signal syscall
pub fn sys_condvar_signal(condvar_id: usize) -> isize {
    trace!("kernel:pid[{}] tid[{}] sys_condvar_signal", get_current_pid(), get_current_tid());
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    
    // Check bounds
    if condvar_id >= process_inner.condvar_list.len() {
        return -1;
    }
    
    let condvar = match process_inner.condvar_list[condvar_id].as_ref() {
        Some(condvar) => Arc::clone(condvar),
        None => return -1,
    };
    
    drop(process_inner);
    condvar.signal();
    0
}
/// condvar wait syscall
pub fn sys_condvar_wait(condvar_id: usize, mutex_id: usize) -> isize {
    trace!("kernel:pid[{}] tid[{}] sys_condvar_wait", get_current_pid(), get_current_tid());
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    
    // Check bounds
    if condvar_id >= process_inner.condvar_list.len() || mutex_id >= process_inner.mutex_list.len() {
        return -1;
    }
    
    let condvar = match process_inner.condvar_list[condvar_id].as_ref() {
        Some(condvar) => Arc::clone(condvar),
        None => return -1,
    };
    
    let mutex = match process_inner.mutex_list[mutex_id].as_ref() {
        Some(mutex) => Arc::clone(mutex),
        None => return -1,
    };
    
    drop(process_inner);
    condvar.wait(mutex);
    0
}
/// enable deadlock detection syscall
///
/// Enable or disable deadlock detection for the current process
pub fn sys_enable_deadlock_detect(enabled: usize) -> isize {
    trace!("kernel:pid[{}] tid[{}] sys_enable_deadlock_detect", get_current_pid(), get_current_tid());
    
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    
    match enabled {
        0 => {
            process_inner.en_deadlock_detect = false;
            0
        }
        1 => {
            process_inner.en_deadlock_detect = true;
            0
        }
        _ => -1, // Invalid parameter
    }
}
