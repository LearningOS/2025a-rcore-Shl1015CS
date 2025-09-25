# Chapter 5 实验报告

## 实验概述

本实验实现了rCore操作系统的第五章内容，主要包括进程创建、stride调度算法以及内存管理系统调用。实验要求实现以下核心功能：

1. **进程创建**：实现`sys_spawn`系统调用，直接创建新进程
2. **stride调度**：实现带优先级的公平调度算法
3. **优先级管理**：实现`sys_set_priority`系统调用
4. **内存管理**：迁移并优化`sys_mmap`、`sys_munmap`、`sys_get_time`系统调用

## 核心功能实现

### 1. sys_spawn系统调用 (syscall ID: 400)

**功能描述**：直接从ELF文件创建新进程，不同于`fork + exec`的组合。

**关键实现**：
```rust
pub fn sys_spawn(path: *const u8) -> isize {
    let token = current_user_token();
    let path = translated_str(token, path);
    
    if let Some(data) = get_app_data_by_name(path.as_str()) {
        let task = current_task().unwrap();
        let new_task = task.spawn(data);
        let new_pid = new_task.pid.0;
        add_task(new_task);
        new_pid as isize
    } else {
        -1
    }
}
```

**技术要点**：
- 直接加载ELF文件创建全新的地址空间
- 不复制父进程的内存内容（区别于fork）
- 正确建立父子进程关系
- 初始化进程默认优先级为16

### 2. Stride调度算法

**算法原理**：
- 每个进程维护当前stride值和优先级
- 每次调度选择stride最小的进程
- 被选中的进程stride += BIG_STRIDE / priority
- 保证CPU时间分配与优先级成正比

**核心实现**：
```rust
pub fn fetch(&mut self) -> Option<Arc<TaskControlBlock>> {
    if self.ready_queue.is_empty() {
        return None;
    }
    
    let mut index = 0;
    let mut min_stride = self.ready_queue[0].inner_exclusive_access().stride;
    
    // 找到stride最小的进程
    for i in 1..self.ready_queue.len() {
        let stride = self.ready_queue[i].inner_exclusive_access().stride;
        // 使用有符号比较处理溢出情况
        if ((stride.wrapping_sub(min_stride)) as i8) < 0 {
            index = i;
            min_stride = stride;
        }
    }
    
    let task = self.ready_queue.remove(index).unwrap();
    
    // 更新被选中进程的stride
    {
        let mut inner = task.inner_exclusive_access();
        let pass = BIG_STRIDE / inner.priority;
        inner.stride = inner.stride.wrapping_add(pass);
    }
    
    Some(task)
}
```

**技术要点**：
- BIG_STRIDE = 255，防止溢出
- 使用wrapping_add处理算术溢出
- 有符号比较正确处理环形数值空间

### 3. sys_set_priority系统调用 (syscall ID: 140)

**功能描述**：设置当前进程的优先级，要求priority >= 2。

**实现**：
```rust
pub fn sys_set_priority(prio: isize) -> isize {
    if prio <= 1 {
        return -1;
    }
    
    let current_task = current_task().unwrap();
    let mut inner = current_task.inner_exclusive_access();
    inner.priority = prio as u8;
    prio
}
```

### 4. 内存管理系统调用

#### sys_mmap实现
**关键修复**：
- 返回值：成功时返回0（rCore约定），而非地址
- 权限检查：支持PROT_READ(1)、PROT_WRITE(2)、PROT_EXEC(4)
- 地址对齐：要求起始地址页对齐
- 冲突检查：确保映射区域未被占用

#### sys_munmap实现
**关键修复**：
- 精确匹配：只能取消映射完整的MapArea边界
- 资源回收：同时从areas向量中移除MapArea并释放页表映射

#### sys_get_time实现
**技术要点**：
- 使用`translated_refmut`安全写入用户空间
- 正确处理TimeVal结构体跨页情况
- 微秒级时间精度

## 数据结构扩展

### TaskControlBlock扩展
```rust
pub struct TaskControlBlockInner {
    // ... 原有字段
    
    /// 进程优先级 (>=2)
    pub priority: u8,
    
    /// 当前stride值
    pub stride: u8,
}
```

## 测试结果

所有测试用例通过：**15/15**

### 关键测试项目：
- ✅ **mmap/munmap测试**：内存映射和取消映射功能正常
- ✅ **spawn测试**：进程创建和执行正确
- ✅ **stride调度测试**：公平性验证通过，执行时间比例符合预期
- ✅ **优先级测试**：set_priority功能正常
- ✅ **时间管理测试**：get_time精度正确
- ✅ **向前兼容性**：所有前章节测试依然通过

### Stride调度公平性验证
测试显示6个不同优先级进程的执行次数大致与优先级成正比：
```
priority = 5,  exitcode = 23223200, ratio = 4644640
priority = 8,  exitcode = 38246400, ratio = 4780800
priority = 6,  exitcode = 28775600, ratio = 4795933
priority = 7,  exitcode = 33348800, ratio = 4764114
priority = 9,  exitcode = 43529600, ratio = 4836622
priority = 10, exitcode = 48566800, ratio = 4856680
```

满足测试要求：max_runtimes/min_runtimes < 1.5

## 技术难点与解决方案

### 1. Stride算法溢出处理
**问题**：u8类型stride值会溢出，影响比较结果
**解决**：使用wrapping arithmetic和有符号比较，将数值空间视为环形

### 2. mmap返回值约定
**问题**：Linux的mmap返回映射地址，但rCore约定返回0
**解决**：通过测试用例分析确定正确的返回值约定

### 3. munmap资源管理
**问题**：只取消页表映射，未释放MapArea导致内存泄漏
**解决**：完整实现资源回收，同时处理页表和内存管理结构

## 总结

本实验成功实现了rCore第五章的所有要求功能：
1. **进程管理**：spawn系统调用提供了fork+exec的替代方案
2. **调度算法**：stride调度实现了带优先级的公平调度
3. **系统调用**：完整的内存管理和时间管理功能
4. **向前兼容**：保持对前章节功能的完全兼容

实验过程中深入理解了进程调度算法、虚拟内存管理和系统调用实现的核心原理，为后续章节的学习奠定了坚实基础。

## 实验环境
- OS: Linux (WSL2)
- Rust版本: nightly
- QEMU版本: qemu-system-riscv64
- 目标架构: riscv64gc-unknown-none-elf
