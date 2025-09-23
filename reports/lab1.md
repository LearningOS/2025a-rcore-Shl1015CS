# Lab1 实验报告：系统调用 sys_trace 的实现

## 实验目标

在 ch3 多任务系统的基础上，实现一个新的系统调用 `sys_trace`（ID 为 410），用于追踪当前任务的系统调用历史信息。该系统调用具有三种功能：内存读取、内存写入和系统调用次数查询。

## 实验要求

实现 `sys_trace` 系统调用，具有以下三种功能：
- `trace_request = 0`: 读取指定地址的一个字节
- `trace_request = 1`: 写入一个字节到指定地址  
- `trace_request = 2`: 查询指定系统调用的调用次数
- 其他值返回 -1

## 实现方案

### 1. 系统调用计数器设计

在 `TaskControlBlock` 中添加系统调用计数器：

```rust
pub struct TaskControlBlock {
    pub task_status: TaskStatus,
    pub task_cx: TaskContext,
    pub syscall_times: [u32; MAX_SYSCALL_NUM],  // 新增
}
```

### 2. 关键实现

#### 2.1 配置常量 (config.rs)
```rust
/// the max syscall number
pub const MAX_SYSCALL_NUM: usize = 512;
```

#### 2.2 任务管理器扩展 (task/mod.rs)
```rust
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
```

#### 2.3 sys_trace 实现 (syscall/process.rs)
```rust
pub fn sys_trace(trace_request: usize, id: usize, data: usize) -> isize {
    trace!("kernel: sys_trace");
    match trace_request {
        0 => {
            // Read a byte from memory at address id
            unsafe {
                let ptr = id as *const u8;
                *ptr as isize
            }
        }
        1 => {
            // Write the low byte of data to memory at address id
            unsafe {
                let ptr = id as *mut u8;
                *ptr = data as u8;
            }
            0
        }
        2 => {
            // Get syscall times for syscall id, including this call
            get_current_syscall_times(id) as isize
        }
        _ => -1,
    }
}
```

#### 2.4 系统调用分发修改 (syscall/mod.rs)
```rust
pub fn syscall(syscall_id: usize, args: [usize; 3]) -> isize {
    // increase syscall times
    increase_syscall_times(syscall_id);
    match syscall_id {
        SYSCALL_WRITE => sys_write(args[0], args[1] as *const u8, args[2]),
        SYSCALL_EXIT => sys_exit(args[0] as i32),
        SYSCALL_YIELD => sys_yield(),
        SYSCALL_GET_TIME => sys_get_time(args[0] as *mut TimeVal, args[1]),
        SYSCALL_TRACE => sys_trace(args[0], args[1], args[2]),
        _ => panic!("Unsupported syscall_id: {}", syscall_id),
    }
}
```

## 实现细节

### 1. 数据结构设计
- 使用 `u32` 数组存储系统调用次数，避免溢出
- 数组大小设为 512，足够容纳系统调用 ID 410
- 在任务初始化时将计数器数组初始化为 0

### 2. 内存操作安全性
- 使用 `unsafe` 块进行内存读写操作
- 将用户空间地址直接转换为指针（符合实验要求的简化设计）
- 读操作返回字节值，写操作只考虑数据的最低字节

### 3. 计数逻辑
- 在 `syscall` 函数入口处统一增加计数
- 确保包含 `sys_trace` 自身的调用
- 对无效的系统调用 ID 进行边界检查

## 测试结果

运行测试命令后的结果：

```
Test passed31765: 7/7
```

### 具体测试表现：
1. ✅ **基础功能测试**: 所有 ch2b 和 ch3b 基础测例通过
2. ✅ **多任务调度**: 任务切换和时间片轮转正常工作
3. ✅ **sys_trace 功能**: 
   - 内存读取功能正常
   - 内存写入功能正常
   - 系统调用计数准确
4. ✅ **输出验证**: "Test trace OK!" 表明追踪功能测试通过

## 关键技术要点

### 1. 系统调用计数的时机
- 在 `syscall` 函数开始时立即计数
- 确保所有系统调用（包括 `sys_trace` 本身）都被正确计数

### 2. 任务隔离
- 每个任务维护独立的系统调用计数器
- 通过 `current_task` 索引访问当前任务的计数器

### 3. 内存访问处理
- 直接使用指针进行内存访问（符合实验简化要求）
- 读操作返回 `isize`，写操作返回 0 表示成功

## 总结

本实验成功实现了 `sys_trace` 系统调用的三项核心功能：

1. **内存读取**: 安全地从用户指定地址读取一个字节
2. **内存写入**: 将指定数据写入用户指定地址
3. **调用统计**: 准确记录和查询系统调用次数

实现过程中合理扩展了任务控制块结构，添加了必要的系统调用计数逻辑，并通过了所有测试用例。代码设计考虑了边界检查和类型安全，在满足实验要求的同时保持了良好的代码质量。

该实现为后续章节的进程管理和地址空间隔离奠定了基础，展示了操作系统内核中系统调用机制的设计和实现方法。
