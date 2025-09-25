//! Process management syscalls
use crate::task::{
    change_program_brk, 
    exit_current_and_run_next, 
    suspend_current_and_run_next,
    current_memory_set_token,
    current_memory_set_translate,
    current_memory_set_insert_framed_area,
    current_memory_set_munmap,
    get_current_syscall_times
};
use crate::mm::{translated_byte_buffer, VirtAddr, VirtPageNum, MapPermission, FRAME_ALLOCATOR};
use crate::timer::get_time_us;

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// task exits and submit an exit code
pub fn sys_exit(_exit_code: i32) -> ! {
    trace!("kernel: sys_exit");
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

/// get time with second and microsecond
/// Reimplement with virtual memory management
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel: sys_get_time");
    let us = get_time_us();
    let time_val = TimeVal {
            sec: us / 1_000_000,
            usec: us % 1_000_000,
        };
    
    // Use translated_byte_buffer to handle virtual memory translation
    let token = current_memory_set_token();
    let buffers = translated_byte_buffer(token, ts as *const u8, core::mem::size_of::<TimeVal>());
    
    // Copy the TimeVal data to user space through translated buffer
    let time_val_bytes = unsafe {
        core::slice::from_raw_parts(&time_val as *const TimeVal as *const u8, core::mem::size_of::<TimeVal>())
    };
    
    let mut offset = 0;
    for buffer in buffers {
        let copy_len = buffer.len().min(time_val_bytes.len() - offset);
        buffer[..copy_len].copy_from_slice(&time_val_bytes[offset..offset + copy_len]);
        offset += copy_len;
    }
    
    0
}

/// trace syscall with virtual memory management and permission checking
pub fn sys_trace(trace_request: usize, id: usize, data: usize) -> isize {
    trace!("kernel: sys_trace");
    
    if trace_request == 0 {
        // Read a byte from memory at address id
        
        // Check if address is within 39-bit limit
        if id > ((1 << 39) - 1) {
            return -1;
        }
        
        let va = VirtAddr::from(id);
        let vpn = va.floor();
        
        // Check if the page is mapped and readable
        if let Some(pte) = current_memory_set_translate(vpn) {
            if !pte.is_valid() || !pte.readable() {
                return -1;
            }
        } else {
            return -1;
        }
        
        // Use translated_byte_buffer to safely read the byte
        let token = current_memory_set_token();
        let buffers = translated_byte_buffer(token, id as *const u8, 1);
        if let Some(buffer) = buffers.first() {
            if !buffer.is_empty() {
                return buffer[0] as isize;
            }
        }
        -1
    } else if trace_request == 1 {
        // Write the low byte of data to memory at address id
        
        // Check if address is within 39-bit limit
        if id > ((1 << 39) - 1) {
            return -1;
        }
        
        let va = VirtAddr::from(id);
        let vpn = va.floor();
        
        // Check if the page is mapped and writable
        if let Some(pte) = current_memory_set_translate(vpn) {
            if !pte.is_valid() || !pte.writable() {
                return -1;
            }
        } else {
            return -1;
        }
        
        // Use translated_byte_buffer to safely write the byte
        let token = current_memory_set_token();
        let mut buffers = translated_byte_buffer(token, id as *const u8, 1);
        if let Some(buffer) = buffers.first_mut() {
            if !buffer.is_empty() {
                buffer[0] = data as u8;
                return 0;
            }
        }
        -1
    } else if trace_request == 2 {
        // Get syscall times for syscall id
        get_current_syscall_times(id) as isize
    } else {
        -1
    }
}

// YOUR JOB: Implement mmap.
pub fn sys_mmap(start: usize, len: usize, prot: usize) -> isize {
    // Check memory page attributes
    if prot & !7 != 0 || prot & 7 == 0 {
        return -1;
    }
    
    let start_va = VirtAddr::from(start);
    if !start_va.aligned() {
        return -1;
    }
    
    let start_vpn = start_va.floor();
    let end_va = VirtAddr::from(start + len);
    let end_vpn = end_va.ceil();
    
    // Check if [start_vpn, end_vpn) has any allocated pages
    for i in start_vpn.0..end_vpn.0 {
        let pte = current_memory_set_translate(VirtPageNum::from(i));
        if let Some(x) = pte {
            if x.is_valid() {
                return -1;
            }
        }
    }
    
    // Check if physical memory is sufficient
    if FRAME_ALLOCATOR.exclusive_access().remain_page_count() < end_vpn.0 - start_vpn.0 {
        return -1;
    }
    
    // Allocate pages
    let mut map_perm = MapPermission::U;
    if prot & 1 == 1 {
        map_perm |= MapPermission::R;
    }
    if prot & 2 == 2 {
        map_perm |= MapPermission::W;
    }
    if prot & 4 == 4 {
        map_perm |= MapPermission::X;
    }
    
    current_memory_set_insert_framed_area(start_va, end_va, map_perm);
    
    0
}

// YOUR JOB: Implement munmap.
pub fn sys_munmap(start: usize, len: usize) -> isize {
    let start_va = VirtAddr::from(start);
    let start_vpn = start_va.floor();
    let end_va = VirtAddr::from(start + len);
    let end_vpn = end_va.ceil();
    
    // Check alignment
    if !start_va.aligned() || !end_va.aligned() {
        return -1;
    }
    
    // Call MemorySet's munmap() to complete releasing
    let result = current_memory_set_munmap(start_vpn, end_vpn);
    if result.is_err() {
        return -1;
    }
    
    0
}
/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel: sys_sbrk");
    if let Some(old_brk) = change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}
