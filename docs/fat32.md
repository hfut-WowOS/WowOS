# FAT32：通用兼容性的文件系统

## 介绍

在数字存储的世界中，有一个文件系统经受住了时间的考验，提供了通用的兼容性和多功能性。FAT32，即文件分配表32，是一种广泛使用的文件系统，自引入以来就在数据存储和传输方面发挥着革命性的作用。在本文中，我们将探讨FAT32的特点、优势和局限性。

FAT32是由微软在1990年代中期开发的，作为FAT16文件系统的继任者。它是一种针对Windows操作系统的磁盘格式，但也被广泛用于其他操作系统和设备上。FAT32的最大优势之一是其通用性，几乎所有现代操作系统都能够读取和写入FAT32格式的磁盘。

FAT32最显著的特点是其对文件和分区大小的支持。相比于其前身FAT16，FAT32能够处理更大容量的硬盘，支持最大达到2TB的分区大小。此外，FAT32还可以处理单个文件的最大大小为4GB，这对于常见的媒体文件和大型数据文件来说已经足够。

另一个FAT32的优势是其简单的文件结构。它使用了一个叫做文件分配表（File Allocation Table）的索引表来记录文件的存储位置和状态。这种简单的结构使得FAT32在各种设备上的实现相对容易，从智能手机和相机到游戏机和嵌入式系统，都可以轻松地使用FAT32格式。

然而，FAT32也有一些局限性。其中之一是对单个文件大小的限制。尽管FAT32可以处理4GB的文件，但对于需要处理超过这一大小的大型文件的用户来说，可能需要考虑其他文件系统，如NTFS或exFAT。此外，FAT32对文件名的支持也有一些限制，比如文件名长度和使用的特殊字符。

尽管有一些限制，FAT32仍然是一种非常有用和广泛应用的文件系统。其通用性和跨平台兼容性使得FAT32成为数据交换和共享的理想选择。无论是在个人电脑、移动设备还是其他电子设备中，FAT32都能提供可靠的存储和传输解决方案。

总之，FAT32是一种经典的文件系统，具有通用的兼容性和简单的文件结构。尽管它有一些限制，但FAT32仍然是许多用户首选的存储格式，为数据的安全存储和高效传输提供了可靠的解决方案。无论是用于个人使用还是商业应用，FAT32都是一个备受青睐的选择。

## 原理

FAT32（File Allocation Table 32）是一种基于磁盘的文件系统，它使用了一种称为文件分配表（File Allocation Table）的数据结构来管理文件的存储和访问。下面是FAT32的工作原理的简要概述：

磁盘分区：首先，磁盘被分成若干个逻辑分区。每个分区都有一个引导扇区（Boot Sector），其中包含了FAT32文件系统的基本信息和启动代码。

文件分配表：在FAT32中，整个分区的空间被划分为多个簇（Cluster）。每个簇是存储文件数据的最小单位。FAT32使用文件分配表来跟踪每个簇的使用情况。文件分配表记录了每个簇的状态（空闲、已使用、坏簇等）以及下一个簇的地址。

根目录区：每个分区还包含一个根目录区，用于存储文件和子目录的索引。根目录区的大小在分区创建时就被确定，并且具有固定的位置。

目录和文件：除了根目录区，FAT32使用目录来组织文件和子目录。每个目录都包含一个目录项表，其中记录了该目录下的文件和子目录的相关信息，如文件名、大小、属性等。

文件存储：当用户创建或复制文件时，FAT32会找到一个连续的空闲簇来存储文件数据。文件数据被分成多个簇进行存储，每个簇的大小通常是4KB。文件分配表记录了每个簇的使用情况和下一个簇的地址，从而形成一个链表结构，通过链表可以遍历整个文件。

文件访问：当需要读取文件时，FAT32根据文件的目录项找到文件的起始簇，并通过链表结构遍历所有簇来读取文件数据。类似地，当需要写入文件时，FAT32会根据文件大小和可用空间找到足够的连续簇来存储新数据，并更新文件分配表和目录项。

通过这种方式，FAT32实现了对文件的存储、访问和管理。它的简单结构和广泛兼容性使得FAT32成为多个操作系统和设备之间交换数据的理想选择。尽管FAT32在一些方面存在限制，如单个文件大小和文件名的长度，但它仍然是一个广泛应用的文件系统，具有普遍的适用性和稳定性。


## 重要的数据结构

```rust
// 定义引导扇区（Boot Sector）的数据结构
#[repr(C, packed)]
struct BootSector {
    jump_instruction: [u8; 3],       // 引导指令
    oem_name: [u8; 8],                // OEM名称
    // 其他引导扇区数据...
}

// 定义文件分配表（File Allocation Table）项的数据结构
#[repr(C, packed)]
struct FATEntry {
    value: u32,                       // 簇的状态和下一个簇的地址
}

// 定义目录项（Directory Entry）的数据结构
#[repr(C, packed)]
struct DirectoryEntry {
    filename: [u8; 8],                // 文件名
    extension: [u8; 3],               // 文件扩展名
    attributes: u8,                   // 文件属性
    // 其他目录项数据...
}

// 定义文件存储的数据结构
struct File {
    start_cluster: u32,               // 文件的起始簇号
    size: u32,                        // 文件的大小
    // 其他文件数据...
}
```

## WowOS 中 fatfs 的实现概述

WowOS 的fat32的文件系统参考了 [rCore-fat](https://github.com/KuangjuX/rCore-fat) 项目，以及往届队伍的部分代码

在内核中，我们使用 OSInode 来表示文件，该结构体包含读写标签、当前偏移、以及对应虚拟文件的引用。对于文件和目录，在内核中都使用 OSInode 来描述，而对于其他可读写对象，例如设备、Pipe则被当作抽象文件处理。

在 rCore-Tutorial 的文件系统中使用 File Trait 来描述抽象文件，并为每种文件类型实现 Trait 中对应的方法，当系统调用操作文件时，则调用 Trait 中对应的方法来操作文件，但对于 FAT32 文件系统来说，对于操作 FAT32 的实际文件来说是不够的，因此我在这里将文件分为了两类，一类为真实的文件，一类则为抽象文件（复用 File Trait）用来描述 Stdio、网卡等设备抽象文件的调用。

在 FAT32 文件系统库的设计中，我们使用 fat_manager 来统一管理 FAT32 文件系统的磁盘内容：

在操作系统启动时， fat32_manager 首先启动文件系统，引导扇区的数据并进行校验。fat_manager 首先会读入 0 号扇区，获得隐藏扇区数并初始化缓存偏移量，之后读取逻辑 0 扇区，即引导扇区，获取 FAT32 的基本信息，随后读取u FSInfo 扇区，获取簇信息，进行签名校验。

当获取文件系统的元信息之后，fat_manager 会根据已有信息计算 FAT 所处的位置，初始化 FAT 结构体，然后根据已有信息生成虚拟根目录项，随后返回 fat_manager 供操作系统调用。


## fatfs 的接口

### 1. block_cache

block_cache 是一个简单的块缓存系统，提供了对磁盘块的读取、修改和写回功能，同时实现了块缓存的管理和替换策略。

- BlockCache 结构体表示单个磁盘块的缓存。它包含了缓存数据、块标识、块设备和修改标志等字段。

- BlockCache 的 new 方法用于从磁盘加载一个块缓存，并初始化相应的字段。

- BlockCache 的 addr_of_offset 方法用于计算缓冲区中指定偏移量的字节地址。

- BlockCache 的 get_ref 方法用于获取缓冲区中指定偏移量的磁盘数据结构的不可变引用。

- BlockCache 的 get_mut 方法用于获取缓冲区中指定偏移量的磁盘数据结构的可变引用。

- BlockCache 的 read 方法和 modify 方法分别用于获取不可变引用和可变引用后执行指定的函数。

- BlockCache 的 sync 方法用于将缓冲区中的内容写回到磁盘块中。

- BlockCache 的 Drop 特性的实现确保在 BlockCache 实例超出作用域时，缓存数据会写回底层块设备。

- BlockCacheManager 结构体表示多个块缓存的管理器，维护一个具有指定限制的块缓存队列，并跟踪起始扇区。

- BlockCacheManager 的 new 方法用于创建一个新的块缓存管理器，并设置初始值。

- BlockCacheManager 的 get_block_cache 方法用于获取一个块缓存，根据块标识和块设备参数返回相应的块- 缓存。

- BlockCacheManager 的 start_sec 方法用于返回起始扇区的值。

- BlockCacheManager 的 set_start_sec 方法用于设置起始扇区的值。

- BlockCacheManager 的 drop_all 方法用于清空块缓存队列。

- BLOCK_CACHE_MANAGER 是一个全局静态变量，用于访问 BlockCacheManager 的实例，提供对块缓存的管理。

- get_block_cache 函数用于外部模块访问文件数据块，返回一个块缓存的可读写锁引用。

- set_start_sec 函数用于设置起始扇区的值。

- write_to_dev 函数用于将所有块缓存的内容写回到磁盘。

### 2. layout

layout 定义了一些重要的数据结构：FAT表、短目录项、长目录项、FAT32文件系统信息、DBR（DOS引导记录）和BPB（BIOS参数块）等，还定义了一些fat32文件系统等数据结构。

FAT表主要内容：

- FAT结构体具有两个字段：fat1_sector表示FAT1的起始扇区，fat2_sector表示FAT2的起始扇区。
new方法用于创建FAT结构体的实例。

- calculate_pos方法用于计算给定簇号在FAT表中对应表项的扇区号和偏移量。

- get_free_cluster方法用于搜索下一个可用的簇。它从当前簇的下一个簇开始，依次查找FAT表项，直到找到一个空闲的簇。

- get_next_cluster方法用于查询给定簇号在FAT表中的下一个簇号。它根据给定的簇号计算对应的FAT表项的位置，并从FAT表中读取该位置的值。

- set_next_cluster方法用于设置给定簇号在FAT表中的下一个簇号。它根据给定的簇号计算对应的FAT表项的位置，并在FAT表中修改该位置的值为下一个簇号。

- get_cluster_at方法用于获取某个簇链中指定索引的簇号。它从给定的起始簇开始，依次获取下一个簇号，直到达到指定的索引位置。
final_cluster方法用于获取某个簇链的最后一个簇号。它从给定的起始簇开始，依次获取下一个簇号，直到遇到结束标记或无效簇号，返回最后一个有效的簇号。

- get_all_cluster_of方法用于获取某个簇链从指定簇开始的所有簇号。它从给定的起始簇开始，依次获取下一个簇号，将每个簇号添加到一个向量中，直到遇到结束标记或无效簇号，返回包含所有簇号的向量。

- count_claster_num方法用于统计某个簇链从指定簇开始到结尾的簇数。它从给定的起始簇开始，依次获取下一个簇号，直到遇到结束标记或超出有效簇号范围，返回簇的数量。

### 3. FAT32Manager

在 FAT32 文件系统库的设计中，我们使用 fat_manager 来统一管理 FAT32 文件系统的磁盘内容：

在操作系统启动时， fat32_manager 首先启动文件系统，引导扇区的数据并进行校验。fat_manager 首先会读入 0 号扇区，获得隐藏扇区数并初始化缓存偏移量，之后读取逻辑 0 扇区，即引导扇区，获取 FAT32 的基本信息，随后读取u FSInfo 扇区，获取簇信息，进行签名校验。

当获取文件系统的元信息之后，fat_manager 会根据已有信息计算 FAT 所处的位置，初始化 FAT 结构体，然后根据已有信息生成虚拟根目录项，随后返回 fat_manager 供操作系统调用。

FAT32Manager结构体包含多个字段，包括块设备的引用(block_device)、文件系统信息扇区的引用(fsinfo)，以及一些属性，如每个簇的扇区数(sectors_per_cluster)、每个扇区的字节数(bytes_per_sector)等。

- FAT32Manager还提供了打开现有的FAT32文件系统的方法(open)，该方法接收一个实现了BlockDevice trait的块设备引用，并返回一个多线程安全引用(Arc<RwLock<Self>>)，用于操作FAT32文件系统。

- FAT32Manager还提供了其他方法，如获取（虚拟）根目录项(get_root_dirent)、分配簇(alloc_cluster)、释放簇(dealloc_cluster)等。

- FAT32Manager结构体内部的私有方法clear_cluster用于清空簇的内容，将其全部写入0。

- get_fat方法返回一个多线程安全引用(Arc<RwLock<FAT>>)，用于获取FAT表。

- size_to_clusters方法将文件大小转换为所需的簇数。它使用每个簇的字节数(bytes_per_cluster)进行计算，并进行取整操作。

- cluster_num_needed方法计算将文件大小扩大到new_size所需的簇数。它根据文件类型(is_dir)和首簇号(first_cluster)进行不同的计算。对于目录，它使用FAT表的计数方法来确定已使用的簇数，然后计算所需的新增簇数。对于文件，它直接使用size_to_clusters方法计算新增的簇数。


- long_name_split函数用于将长文件名拆分成字符串数组。它根据指定的长度限制(LONG_NAME_LEN)将长文件名切割成多个部分，每个部分占据一个目录项。

- split_name_ext函数用于将文件名和扩展名分割开。它根据.进行分割，并返回分割后的文件名和扩展名。

- short_name_format函数用于将短文件名格式化为目录项存储的内容。它将文件名和扩展名分别转换为固定长度的数组，并进行大小写转换。

- generate_short_name函数用于由长文件名生成短文件名。它根据指定规则生成一个长度为11的短文件名，包括前6个字符、"~1"、扩展名。

### 4. vfs

vfs 是一个用于表示虚拟文件系统中文件的结构体 VFile 的实现。它包含了文件的相关信息和对文件系统的引用，以便进行文件操作。

结构体定义部分包括以下字段：

- name: String：文件名

- short_sector: usize：文件短目录项所在扇区

- short_offset: usize：文件短目录项所在扇区的偏移

- long_pos_vec: Vec<(usize, usize)>：长目录项的位置（扇区和偏移）

- attribute: u8：文件属性

- fs: Arc<RwLock<FAT32Manager>>：文件系统引用

- block_device: Arc<dyn BlockDevice>：块设备引用

实现部分包括以下方法：

- new：用于创建 VFile 结构体的实例

- name：获取文件名

- file_size：获取文件大小

- is_dir：判断文件是否为目录

- is_short：判断文件是否为短文件名

- read_short_dirent：读取文件的短目录项

- modify_long_dirent：修改文件的长目录项

- modify_short_dirent：修改文件的短目录项

- get_pos：获取文件偏移量所在的扇区和偏移

- set_first_cluster：设置文件的首簇号

- first_cluster：获取文件的首簇号

- find_long_name：根据长文件名查找文件

- find_short_name：根据短文件名查找文件

- find_vfile_byname: 根据文件名在当前目录下搜索文件，并返回对应的文件对象。

- find_vfile_bypath: 根据路径递归搜索文件，并返回对应的文件对象。

- increase_size: 对文件进行扩容，根据新的文件大小计算需要的簇数，并进行簇的分配和链接。

- create: 在当前目录下创建文件，包括短文件名和长文件名的处理，以及目录的创建。

- ls: 列出当前目录下的文件和目录，返回一个包含文件名和属性的列表。

- read_at: 在指定偏移量处读取文件数据。

- write_at: 在指定偏移量处写入数据，并根据数据大小扩容文件。

- clear: 清空文件的目录项和簇，并释放文件占用的簇。

- find_free_dirent: 查找可用的目录项，返回偏移量。

- remove: 删除文件的目录项和簇，并返回释放的簇数。

- stat: 获取文件的信息，包括大小、块大小、块数、是否为目录和时间等。

- set_time: 设置文件的时间信息。

- dirent_info: 获取指定偏移量的目录项的信息，包括名称、偏移量、簇号和属性。

- create_root_vfile: 创建根目录的虚拟文件。