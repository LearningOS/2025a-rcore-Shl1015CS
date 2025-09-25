# rCore Ch4 实验报告：虚拟内存管理

## 实验概述

本实验主要实现了基于SV39页式虚拟内存管理系统，包括虚拟内存映射、页表管理、系统调用扩展等核心功能。

### 实验目标
- 实现虚拟内存管理机制
- 完善系统调用功能（sys_get_time, sys_trace, sys_mmap, sys_munmap）
- 实现页表查找和地址转换
- 添加系统调用统计功能

## 主要实现内容

### 1. 系统调用实现

#### 1.1 sys_get_time
```rust
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    let us = get_time_us();
    let time_val = TimeVal {
        sec: us / 1_000_000,
        usec: us % 1_000_000,
    };
    
    // 使用translated_byte_buffer处理跨页情况
    let token = current_memory_set_token();
    let buffers = translated_byte_buffer(token, ts as *const u8, core::mem::size_of::<TimeVal>());
    
    // 将时间数据安全地复制到用户空间
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
```

**设计要点**：
- 使用`translated_byte_buffer`处理用户虚拟地址到物理地址的转换
- 正确处理跨页的TimeVal结构体
- 确保内存访问安全

#### 1.2 sys_trace
```rust
pub fn sys_trace(trace_request: usize, id: usize, data: usize) -> isize {
    if trace_request == 0 {
        // 读取内存字节
        if id > ((1 << 39) - 1) {
            return -1; // 39位地址空间限制检查
        }
        
        let va = VirtAddr::from(id);
        let vpn = va.floor();
        
        // 检查页面是否映射且可读
        if let Some(pte) = current_memory_set_translate(vpn) {
            if !pte.is_valid() || !pte.readable() {
                return -1;
            }
        } else {
            return -1;
        }
        
        let token = current_memory_set_token();
        let buffers = translated_byte_buffer(token, id as *const u8, 1);
        if let Some(buffer) = buffers.first() {
            if !buffer.is_empty() {
                return buffer[0] as isize;
            }
        }
        -1
    } else if trace_request == 1 {
        // 写入内存字节（类似逻辑）
        // ...
    } else if trace_request == 2 {
        // 获取系统调用次数
        get_current_syscall_times(id) as isize
    } else {
        -1
    }
}
```

**设计要点**：
- 添加39位地址空间限制检查（RISC-V SV39特性）
- 严格的权限检查（valid、readable、writable）
- 支持系统调用统计查询功能

#### 1.3 sys_mmap
```rust
pub fn sys_mmap(start: usize, len: usize, prot: usize) -> isize {
    // 检查内存页属性
    if prot & !7 != 0 || prot & 7 == 0 {
        return -1;
    }
    
    let start_va = VirtAddr::from(start);
    if !start_va.aligned() {
        return -1; // 页对齐检查
    }
    
    let start_vpn = start_va.floor();
    let end_va = VirtAddr::from(start + len);
    let end_vpn = end_va.ceil();
    
    // 检查[start_vpn, end_vpn)是否有已分配页面
    for i in start_vpn.0..end_vpn.0 {
        let pte = current_memory_set_translate(VirtPageNum::from(i));
        if let Some(x) = pte {
            if x.is_valid() {
                return -1;
            }
        }
    }
    
    // 检查物理内存是否充足
    if FRAME_ALLOCATOR.exclusive_access().remain_page_count() < end_vpn.0 - start_vpn.0 {
        return -1;
    }
    
    // 设置页面权限
    let mut map_perm = MapPermission::U;
    if prot & 1 == 1 { map_perm |= MapPermission::R; }
    if prot & 2 == 2 { map_perm |= MapPermission::W; }
    if prot & 4 == 4 { map_perm |= MapPermission::X; }
    
    current_memory_set_insert_framed_area(start_va, end_va, map_perm);
    0
}
```

**设计要点**：
- 严格的参数检查（页对齐、权限位合法性）
- 物理内存充足性检查，避免分配失败
- 正确的权限设置（R/W/X + U位）

#### 1.4 sys_munmap
```rust
pub fn sys_munmap(start: usize, len: usize) -> isize {
    let start_va = VirtAddr::from(start);
    let start_vpn = start_va.floor();
    let end_va = VirtAddr::from(start + len);
    let end_vpn = end_va.ceil();
    
    // 检查对齐
    if !start_va.aligned() || !end_va.aligned() {
        return -1;
    }
    
    // 调用MemorySet的munmap()完成释放
    let result = current_memory_set_munmap(start_vpn, end_vpn);
    if result.is_err() {
        return -1;
    }
    
    0
}
```

### 2. 内存管理扩展

#### 2.1 MemorySet::munmap实现
```rust
pub fn munmap(&mut self, start_vpn: VirtPageNum, end_vpn: VirtPageNum) -> Result<(), ()> {
    for (index, area) in self.areas.iter().enumerate() {
        if area.vpn_range.get_start() == start_vpn && area.vpn_range.get_end() == end_vpn {
            let mut area = self.areas.remove(index);
            area.unmap(&mut self.page_table);
            return Ok(());
        }
    }
    Err(())
}
```

**设计特点**：
- **精确匹配**：只有当请求的VPN范围与某个MapArea完全匹配时才能成功
- 符合rCore实验要求：不处理交叉、截断区间的情况
- 返回Result类型，明确表示操作成功与否

#### 2.2 页表查找功能扩展
```rust
// PageTable中添加public方法
pub fn find_pte(&self, vpn: VirtPageNum) -> Option<&mut PageTableEntry> {
    // 页表遍历逻辑
}

// MemorySet中添加封装
pub fn find_pte(&self, vpn: VirtPageNum) -> Option<&mut PageTableEntry> {
    self.page_table.find_pte(vpn)
}
```

#### 2.3 物理内存管理改进
```rust
impl StackFrameAllocator {
    pub fn remain_page_count(&self) -> usize {
        self.recycled.len() + (self.end - self.current)
    }
}
```

### 3. 系统调用统计功能

#### 3.1 统计机制
```rust
// 在syscall入口处添加统计
pub fn syscall(syscall_id: usize, args: [usize; 3]) -> isize {
    increase_syscall_times(syscall_id); // 每次系统调用都计数
    
    match syscall_id {
        // 系统调用分发
    }
}
```

#### 3.2 统计数据存储
- 在TaskControlBlock中使用数组存储每个任务的系统调用次数
- 支持通过sys_trace查询特定系统调用的调用次数

## 关键技术点

### 1. 虚拟内存地址转换
- 使用`translated_byte_buffer`处理用户空间到内核空间的数据传输
- 正确处理跨页的数据结构
- 支持不连续的物理页面映射

### 2. 页表管理
- 基于SV39的三级页表结构
- 页表项权限位的正确设置和检查
- 页面分配和回收的正确实现

### 3. 内存安全
- 严格的地址边界检查
- 权限验证（readable/writable/executable）
- 物理内存充足性检查

## 遇到的问题和解决方案

### 1. 编译错误：未使用的导入
**问题**：导入了PTEFlags和PAGE_SIZE但未使用
**解决**：删除未使用的导入，保持代码整洁

### 2. munmap精确匹配问题
**问题**：最初实现允许部分匹配，导致测试失败
**解决**：改为精确匹配MapArea边界，符合实验要求

### 3. 39位地址空间限制
**问题**：sys_trace未检查地址边界，可能访问非法地址
**解决**：添加 `if id > ((1 << 39) - 1)` 检查

### 4. 物理内存检查缺失
**问题**：mmap分配时未检查物理内存是否充足
**解决**：使用`remain_page_count()`预先检查内存可用性

## 总结

本实验成功实现了完整的虚拟内存管理系统，包括：

1. **系统调用扩展**：完善了时间获取、内存跟踪、内存映射等关键系统调用
2. **虚拟内存管理**：实现了mmap/munmap的完整功能
3. **安全机制**：添加了多层次的安全检查
4. **统计功能**：实现了系统调用统计机制

通过本实验，深入理解了操作系统虚拟内存管理的核心机制，掌握了页表操作、地址转换、内存映射等关键技术。实验中遇到的问题也加深了对内存管理细节的理解，特别是精确匹配和边界检查的重要性。
