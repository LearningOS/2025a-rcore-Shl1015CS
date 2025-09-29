//! File and filesystem-related syscalls
use core::mem::size_of;

use crate::fs::{link_file, open_file, stat_file, unlink_file, OpenFlags, Stat};
use crate::mm::{translated_byte_buffer, translated_str, UserBuffer};
use crate::task::{current_task, current_user_token};
use super::copy_to_user;
pub fn sys_write(fd: usize, buf: *const u8, len: usize) -> isize {
    let task = current_task().unwrap();
    trace!("kernel:pid[{}] sys_write", task.pid.0);
    let token = task.get_user_token();
    
    let file = {
        let inner = task.inner_exclusive_access();
        if fd >= inner.fd_table.len() {
            return -1;
        }
        match &inner.fd_table[fd] {
            Some(file) if file.writable() => file.clone(),
            Some(_) => return -1,  // File exists but not writable
            None => return -1,     // No file at this fd
        }
    }; // inner lock released here
    
    file.write(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
}

pub fn sys_read(fd: usize, buf: *const u8, len: usize) -> isize {
    let task = current_task().unwrap();
    trace!("kernel:pid[{}] sys_read", task.pid.0);
    let token = task.get_user_token();
    
    let file = {
        let inner = task.inner_exclusive_access();
        if fd >= inner.fd_table.len() {
            return -1;
        }
        match &inner.fd_table[fd] {
            Some(file) if file.readable() => file.clone(),
            Some(_) => return -1,  // File exists but not readable
            None => return -1,     // No file at this fd
        }
    }; // inner lock released here
    
    trace!("kernel: sys_read .. file.read");
    file.read(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
}

pub fn sys_open(path: *const u8, flags: u32) -> isize {
    let task = current_task().unwrap();
    trace!("kernel:pid[{}] sys_open", task.pid.0);
    let token = task.get_user_token();
    let path = translated_str(token, path);
    
    if let Some(inode) = open_file(path.as_str(), OpenFlags::from_bits(flags).unwrap()) {
        let mut inner = task.inner_exclusive_access();
        let fd = inner.alloc_fd();
        inner.fd_table[fd] = Some(inode);
        if fd >= inner.fd_name.len() {
            inner.fd_name.resize(fd + 1, None);
        }
        inner.fd_name[fd] = Some(path);
        fd as isize
    } else {
        -1
    }
}

pub fn sys_close(fd: usize) -> isize {
    let task = current_task().unwrap();
    trace!("kernel:pid[{}] sys_close", task.pid.0);
    
    let mut inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() || inner.fd_table[fd].is_none() {
        return -1;
    }
    
    inner.fd_table[fd].take();
    if fd < inner.fd_name.len() {
        inner.fd_name[fd].take();
    }
    0
}

/// YOUR JOB: Implement fstat.
pub fn sys_fstat(_fd: usize, _st: *mut Stat) -> isize {
    trace!(
        "kernel:pid[{}] sys_fstat NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    let current = current_task().unwrap();
    let inner = current.inner_exclusive_access();
    if inner.fd_table[_fd].is_none() {
        return -1;
    }
    let stat = stat_file(inner.fd_name[_fd].clone().unwrap().as_str());
    drop(inner);
    drop(current);
    if let Some(stat) = stat{
        copy_to_user(&stat as *const Stat as usize, _st as * const u8, size_of::<Stat>());
        0
    }
    else{
        -1
    }
}

/// YOUR JOB: Implement linkat.
pub fn sys_linkat(_old_name: *const u8, _new_name: *const u8) -> isize {
    trace!(
        "kernel:pid[{}] sys_linkat NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    let token = current_user_token();
    let old_path = translated_str(token, _old_name);
    let new_path = translated_str(token, _new_name);
    if link_file(old_path.as_str(), new_path.as_str()) {
        0
    } else {
        -1
    }
}

/// YOUR JOB: Implement unlinkat.
pub fn sys_unlinkat(_name: *const u8) -> isize {
    trace!(
        "kernel:pid[{}] sys_unlinkat NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    let token = current_user_token();
    let path = translated_str(token, _name);
    if unlink_file(path.as_str()) {
        0
    } else {
        -1
    }
}