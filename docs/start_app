# 向内核中添加 initproc 和 user_shell

```assembly
# os/src/start_app.asm
.align 3
.section .data
.global _num_app
_num_app:
    .quad 2
    .quad app_0_start
    .quad app_1_start
    .quad app_1_end
```

这一部分将后续的数据按照 2^3（8 字节）的边界进行对齐。它声明了一个全局符号 _num_app，并将其赋值为 2。接下来的三行定义了四个四倍字（8 字节）的值。这些值代表了两个应用程序二进制文件的起始和结束地址：`app_0_start`、`app_1_start` 和 `app_1_end`。

```assembly
.section .data
.global _app_names
_app_names:
    .string "initproc"
    .string "user_shell"
```

这一部分定义了一个全局符号 `_app_names`。接下来的两行定义了两个以空字符结尾的字符串："initproc" 和 "user_shell"。

```assembly
.section .data
.global app_0_start
.global app_0_end
.align 3
app_0_start:
    .incbin "start_apps/initproc"
app_0_end:
```

这一部分定义了 `app_0` 的起始和结束地址。它将后续的数据按照 8 字节边界对齐，并声明了两个全局符号：`app_0_start` 和 `app_0_end`。`app_0_start` 标签表示应用程序代码的开始，`.incbin` 指令将二进制代码从文件中包含进来。`app_0_end` 标签用于标记应用程序代码的结束。

```assembly
.section .data
.global app_1_start
.global app_1_end
.align 3
app_1_start:
    .incbin "start_apps/user_shell"
app_1_end:
```

实现同上。

在main函数中，调用了 `task::add_initproc();` 来加载 `initproc` 。

```rust
// os/src/task/mod.rs

lazy_static! {
    pub static ref INITPROC: Arc<ProcessControlBlock> = {
        let inode = open("/", "initproc", OpenFlags::O_RDONLY).unwrap();
        let v = inode.read_all();
        ProcessControlBlock::new(v.as_slice())
    };
}

pub fn add_initproc() {
    let _initproc = INITPROC.clone();
}

```
