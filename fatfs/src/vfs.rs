use super::{fat32_manager::*, get_block_cache, layout::*, BlockDevice};
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::RwLock;

/// 表示虚拟文件系统中的一个文件
/// 该结构体用于表示虚拟文件系统中的文件，包含了文件的相关信息和对文件系统的引用，以便进行文件操作。
#[derive(Clone, Debug)]
pub struct VFile {
    name: String,                       // 文件名
    short_sector: usize,                // 文件短目录项所在扇区
    short_offset: usize,                // 文件短目录项所在扇区的偏移
    long_pos_vec: Vec<(usize, usize)>,  // 长目录项的位置<sector, offset>
    attribute: u8,                      // 文件属性
    fs: Arc<RwLock<FAT32Manager>>,      // 文件系统引用
    block_device: Arc<dyn BlockDevice>, // 块设备引用
}

impl VFile {
    pub fn new(
        name: String,
        short_sector: usize,
        short_offset: usize,
        long_pos_vec: Vec<(usize, usize)>,
        attribute: u8,
        fs: Arc<RwLock<FAT32Manager>>,
        block_device: Arc<dyn BlockDevice>,
    ) -> Self {
        Self {
            name,
            short_sector,
            short_offset,
            long_pos_vec,
            attribute,
            fs,
            block_device,
        }
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn file_size(&self) -> u32 {
        self.read_short_dirent(|se: &ShortDirEntry| se.file_size())
    }

    pub fn is_dir(&self) -> bool {
        if 0 != (self.attribute & ATTR_DIRECTORY) {
            true
        } else {
            false
        }
    }

    pub fn is_short(&self) -> bool {
        if self.long_pos_vec.len() == 0 {
            true
        } else {
            false
        }
    }

    fn read_short_dirent<V>(&self, f: impl FnOnce(&ShortDirEntry) -> V) -> V {
        if self.short_sector == 0 {
            // 根目录项
            let root_dirent = self.fs.read().get_root_dirent();
            let rr = root_dirent.read();
            f(&rr)
        } else {
            get_block_cache(self.short_sector, self.block_device.clone())
                .read()
                .read(self.short_offset, f)
        }
    }

    fn modify_long_dirent<V>(&self, index: usize, f: impl FnOnce(&mut LongDirEntry) -> V) -> V {
        let (sector, offset) = self.long_pos_vec[index];
        get_block_cache(sector, self.block_device.clone()).write().modify(offset, f)
    }

    fn modify_short_dirent<V>(&self, f: impl FnOnce(&mut ShortDirEntry) -> V) -> V {
        if self.short_sector == 0 {
            let root_dirent = self.fs.read().get_root_dirent();
            let mut rw = root_dirent.write();
            f(&mut rw)
        } else {
            get_block_cache(self.short_sector, self.block_device.clone())
                .write()
                .modify(self.short_offset, f)
        }
    }

    /// 获取文件偏移量所在的扇区和偏移
    fn get_pos(&self, offset: usize) -> (usize, usize) {
        let (_, section, offset) = self.read_short_dirent(|short_entry: &ShortDirEntry| {
            short_entry.get_pos(offset, &self.fs, &self.fs.read().get_fat(), &self.block_device)
        });
        (section, offset)
    }

    pub fn set_first_cluster(&self, clu: u32) {
        self.modify_short_dirent(|se: &mut ShortDirEntry| {
            se.set_first_cluster(clu);
        })
    }
    pub fn first_cluster(&self) -> u32 {
        self.read_short_dirent(|se: &ShortDirEntry| se.first_cluster())
    }

    fn find_long_name(&self, name: &str, dir_ent: &ShortDirEntry) -> Option<VFile> {
        // 拆分长文件名
        let name_vec = long_name_split(name);
        let long_ent_num = name_vec.len();
        let mut offset: usize = 0;
        let mut long_entry = LongDirEntry::new();

        let mut long_pos_vec: Vec<(usize, usize)> = Vec::new();
        let name_last = name_vec[long_ent_num - 1].clone();

        loop {
            long_pos_vec.clear();
            // 读取offset处的目录项
            let mut read_size = dir_ent.read_at(
                offset,
                long_entry.as_bytes_mut(),
                &self.fs,
                &self.fs.read().get_fat(),
                &self.block_device,
            );
            if read_size != DIRENT_SZ || long_entry.is_empty() {
                return None;
            }
            // 先匹配最后一个长文件名目录项，即长文件名的最后一块
            if long_entry.attr() == ATTR_LONG_NAME && long_entry.get_name_raw() == name_last{
                // 如果名称一致，则获取 order进行下一步校验
                let mut order = long_entry.order();
                // 校验 order的合法性，不合法则跳过继续搜索
                if order & 0x40 == 0 || order == 0xE5 {
                    offset += DIRENT_SZ;
                    continue;
                }
                // 恢复 order为正确的次序值
                order = order ^ 0x40;
                // 如果长文件名目录项数量对不上，则跳过继续搜索
                if order as usize != long_ent_num {
                    offset += DIRENT_SZ;
                    continue;
                }
                // 如果order匹配通过，开一个循环继续匹配长名目录项
                let mut is_match = true;
                for i in 1..order as usize {
                    read_size = dir_ent.read_at(
                        offset + i * DIRENT_SZ,
                        long_entry.as_bytes_mut(),
                        &self.fs,
                        &self.fs.read().get_fat(),
                        &self.block_device,
                    );
                    if read_size != DIRENT_SZ {
                        return None;
                    }
                    // 匹配前一个名字段，如果失败就退出
                    if long_entry.get_name_raw() != name_vec[long_ent_num - 1 - i] || long_entry.attr() != ATTR_LONG_NAME {
                        is_match = false;
                        break;
                    }
                }
                if is_match {
                    // 如果成功，读短目录项，进行校验
                    let checksum = long_entry.check_sum();
                    let mut short_entry = ShortDirEntry::new();
                    let short_entry_offset = offset + long_ent_num * DIRENT_SZ;
                    read_size = dir_ent.read_at(
                        short_entry_offset,
                        short_entry.as_bytes_mut(),
                        &self.fs,
                        &self.fs.read().get_fat(),
                        &self.block_device,
                    );
                    if read_size != DIRENT_SZ {
                        return None;
                    }
                    if short_entry.is_valid() && checksum == short_entry.checksum() {
                        let (short_sector, short_offset) = self.get_pos(short_entry_offset);
                        for i in 0..order as usize {
                            // 存入长名目录项位置了，第一个在栈顶
                            let pos = self.get_pos(offset + i * DIRENT_SZ);
                            long_pos_vec.push(pos);
                        }
                        return Some(VFile::new(
                            String::from(name),
                            short_sector,
                            short_offset,
                            long_pos_vec,
                            short_entry.attr(),
                            self.fs.clone(),
                            self.block_device.clone(),
                        ));
                    } else {
                        panic!("Simple-Fat32: short_entry is not valid or checksum wrong")
                    }
                }
            }
            offset += DIRENT_SZ;
        }
    }

    fn find_short_name(&self, name: &str, dir_ent: &ShortDirEntry) -> Option<VFile> {
        let name_upper = name.to_ascii_uppercase();
        let mut short_entry = ShortDirEntry::new();
        let mut offset = 0;
        let mut read_size: usize;
        loop {
            read_size = dir_ent.read_at(
                offset,
                short_entry.as_bytes_mut(),
                &self.fs,
                &self.fs.read().get_fat(),
                &self.block_device,
            );
            if read_size != DIRENT_SZ || short_entry.is_empty() {
                return None;
            } else {
                // 判断名字是否一样
                // println!("name_upper:{}, entry name:{}",name_upper,short_entry.get_name_uppercase());
                if short_entry.is_valid() && name_upper == short_entry.get_name_uppercase() {
                    let (short_sector, short_offset) = self.get_pos(offset);
                    let long_pos_vec: Vec<(usize, usize)> = Vec::new();
                    return Some(VFile::new(
                        String::from(name),
                        short_sector,
                        short_offset,
                        long_pos_vec,
                        short_entry.attr(),
                        self.fs.clone(),
                        self.block_device.clone(),
                    ));
                } else {
                    offset += DIRENT_SZ;
                    continue;
                }
            }
        }
    }

    /// 根据名称搜索当前目录下的文件
    fn find_vfile_byname(&self, name: &str) -> Option<VFile> {
        // 不是目录则退出
        assert!(self.is_dir());
        let (name_, ext_) = split_name_ext(name);
        self.read_short_dirent(|short_entry: &ShortDirEntry| {
            if name_.len() > 8 || ext_.len() > 3 {
                //长文件名
                return self.find_long_name(name, short_entry);
            } else {
                // 短文件名
                return self.find_short_name(name, short_entry);
            }
        })
    }

    /// 根据路径递归搜索文件
    pub fn find_vfile_bypath(&self, path: Vec<&str>) -> Option<Arc<VFile>> {
        let len = path.len();
        if len == 0 {
            return Some(Arc::new(self.clone()));
        }
        let mut current_vfile = self.clone();
        for i in 0..len {
            if path[i] == "" || path[i] == "." {
                continue;
            }
            if let Some(vfile) = current_vfile.find_vfile_byname(path[i]) {
                current_vfile = vfile;
            } else {
                return None;
            }
        }
        Some(Arc::new(current_vfile))
    }

    /// 对文件进行扩容，new_size 是文件当前偏移量加 buf 长度
    fn increase_size(&self, new_size: u32) {
        let first_cluster = self.first_cluster();
        let old_size = self.file_size();
        // 检查需要扩容的新大小new_size是否小于等于当前文件大小，如果是，则表示写入范围不会超出现有文件范围，直接返回，不执行扩容操作。
        if new_size <= old_size {
            return;
        }
        let manager_writer = self.fs.write();
        // 传给 fat32_manager 来计算需要多少簇
        let needed = manager_writer.cluster_num_needed(old_size, new_size, self.is_dir(), first_cluster);
        if needed == 0 {
            // 如果needed为0，表示不需要扩容，如果文件是普通文件（非目录）
            // 则将文件大小设置为new_size。
            if !self.is_dir() {
                self.modify_short_dirent(|short_entry: &mut ShortDirEntry| {
                    short_entry.set_file_size(new_size);
                });
            }
            return;
        }

        // 需要扩容
        if let Some(cluster) = manager_writer.alloc_cluster(needed) {
            if first_cluster == 0 {
                // 从未分配过簇的情况（例如新文件？）
                drop(manager_writer);
                // 将第一个簇号设置为新分配的簇号cluster
                self.modify_short_dirent(|short_entry: &mut ShortDirEntry| {
                    // 貌似失败了
                    short_entry.set_first_cluster(cluster);
                });
            } else {
                // 已经分配簇
                // 获取文件系统的fat对象，获取其写锁fat_writer，通过调用fat_writer.final_cluster方法，传入第一个簇号和设备引用，找到最后一个簇的簇号final_cluster
                let fat = manager_writer.get_fat();
                let fat_writer = fat.write();
                // 找到最后一个簇
                let final_cluster = fat_writer.final_cluster(first_cluster, self.block_device.clone());
                assert_ne!(cluster, 0);
                // 设置 FAT 表进行链接
                fat_writer.set_next_cluster(final_cluster, cluster, self.block_device.clone());
                // 整个扩容操作完成后，释放文件系统的写锁manager_writer
                drop(manager_writer);
            }
            // 更新文件大小
            self.modify_short_dirent(|short_entry: &mut ShortDirEntry| {
                short_entry.set_file_size(new_size);
            });
        } else {
            // 无法分配所需的簇，则抛出错误信息，表示扩容失败。
            panic!("[DEBUG] Simple-FAT32: increase size failed! Out of cluster!");
        }
    }

    /// 在当前目录下创建文件
    pub fn create(&self, name: &str, attribute: u8) -> Option<Arc<VFile>> {
        // 检测同名文件
        assert!(self.is_dir());
        let (name_, ext_) = split_name_ext(name);
        // 搜索空处
        let mut dirent_offset: usize;
        if let Some(offset) = self.find_free_dirent() {
            dirent_offset = offset;
        } else {
            return None;
        }
        // 定义一个空的短文件名目录项用于写入
        let mut tmp_short_ent = ShortDirEntry::new();
        if name_.len() > 8 || ext_.len() > 3 {
            // 长文件名
            // 生成短文件名及对应目录项
            let short_name = generate_short_name(name);
            let (_name, _ext) = short_name_format(short_name.as_str());
            tmp_short_ent.initialize(&_name, &_ext, attribute);

            // 长文件名拆分
            let mut v_long_name = long_name_split(name);
            let long_ent_num = v_long_name.len(); // 需要创建的长文件名目录项个数

            // 定义一个空的长文件名目录项用于写入
            let mut tmp_long_ent = LongDirEntry::new();
            // 逐个写入长名目录项
            for i in 0..long_ent_num {
                // 按倒序填充长文件名目录项，目的是为了避免名字混淆
                let mut order: u8 = (long_ent_num - i) as u8;
                if i == 0 {
                    // 最后一个长文件名目录项，将该目录项的序号与 0x40 进行或运算然后写入
                    order |= 0x40;
                }
                // 初始化长文件名目录项
                tmp_long_ent.initialize(v_long_name.pop().unwrap().as_bytes(), order, tmp_short_ent.checksum());
                // 写入长文件名目录项
                assert_eq!(self.write_at(dirent_offset, tmp_long_ent.as_bytes_mut()), DIRENT_SZ);
                // 更新写入位置
                dirent_offset += DIRENT_SZ;
            }
        } else {
            // 短文件名
            // todo: 短文件名也会生成一个长文件名目录项，用于存储大小写
            let (_name, _ext) = short_name_format(name);
            tmp_short_ent.initialize(&_name, &_ext, attribute);
            tmp_short_ent.set_case(0x8); // 全部小写
        }
        // 写短目录项（长文件名也是有短文件名目录项的）
        assert_eq!(self.write_at(dirent_offset, tmp_short_ent.as_bytes_mut()), DIRENT_SZ);
        // 这边的 if let 算是一个验证
        if let Some(vfile) = self.find_vfile_byname(name) {
            // 如果是目录类型，需要创建.和..
            if attribute & ATTR_DIRECTORY != 0 {
                // 先写入 .. 使得目录获取第一个簇
                let (_name, _ext) = short_name_format("..");
                let mut par_dir = ShortDirEntry::new();
                par_dir.initialize(&_name, &_ext, ATTR_DIRECTORY);
                par_dir.set_first_cluster(self.first_cluster());
                vfile.write_at(DIRENT_SZ, par_dir.as_bytes_mut());

                let (_name, _ext) = short_name_format(".");
                let mut self_dir = ShortDirEntry::new();
                self_dir.initialize(&_name, &_ext, ATTR_DIRECTORY);
                self_dir.set_first_cluster(vfile.first_cluster());
                vfile.write_at(0, self_dir.as_bytes_mut());
            }
            return Some(Arc::new(vfile));
        } else {
            None
        }
    }

    // ls，返回二元组，第一个是文件名，第二个是文件属性（文件或者目录）
    // todo：使用 dirent_info 方法
    pub fn ls(&self) -> Option<Vec<(String, u8)>> {
        if !self.is_dir() {
            return None;
        }
        let mut list: Vec<(String, u8)> = Vec::new();
        let mut file_entry = LongDirEntry::new();
        let mut offset = 0;
        loop {
            let read_size = self.read_short_dirent(|curr_ent: &ShortDirEntry| {
                curr_ent.read_at(
                    offset,
                    file_entry.as_bytes_mut(),
                    &self.fs,
                    &self.fs.read().get_fat(),
                    &self.block_device,
                )
            });
            // 读取完了
            if read_size != DIRENT_SZ || file_entry.is_empty() {
                return Some(list);
            }
            // 文件被标记删除则跳过
            if file_entry.is_deleted() {
                offset += DIRENT_SZ;
                continue;
            }
            // 注意：Linux中文件创建都会创建一个长文件名目录项，用于处理文件大小写问题
            if file_entry.attr() != ATTR_LONG_NAME {
                // 短文件名
                let (_, se_array, _) = unsafe { file_entry.as_bytes_mut().align_to_mut::<ShortDirEntry>() };
                let short_entry = se_array[0];
                list.push((short_entry.get_name_lowercase(), short_entry.attr()));
            } else {
                // 长文件名
                // 如果是长文件名目录项，则必是长文件名最后的那一段
                let mut name = String::new();
                let order = file_entry.order() ^ 0x40;
                for _ in 0..order {
                    name.insert_str(0, file_entry.get_name_format().as_str());
                    offset += DIRENT_SZ;
                    let read_size = self.read_short_dirent(|curr_ent: &ShortDirEntry| {
                        curr_ent.read_at(
                            offset,
                            file_entry.as_bytes_mut(),
                            &self.fs,
                            &self.fs.read().get_fat(),
                            &self.block_device,
                        )
                    });
                    if read_size != DIRENT_SZ || file_entry.is_empty() {
                        panic!("ls read long name entry error!");
                    }
                }
                list.push((name.clone(), file_entry.attr()));
            }
            offset += DIRENT_SZ;
        }
    }

    pub fn read_at(&self, offset: usize, buf: &mut [u8]) -> usize {
        self.read_short_dirent(|short_entry: &ShortDirEntry| {
            short_entry.read_at(offset, buf, &self.fs, &self.fs.read().get_fat(), &self.block_device)
        })
    }

    pub fn write_at(&self, offset: usize, buf: &[u8]) -> usize {
        // 需要提前扩容
        self.increase_size((offset + buf.len()) as u32);
        self.modify_short_dirent(|short_entry: &mut ShortDirEntry| {
            short_entry.write_at(offset, buf, &self.fs, &self.fs.read().get_fat(), &self.block_device)
        })
    }

    pub fn clear(&self) {
        let first_cluster: u32 = self.first_cluster();
        // 检查文件是否为目录文件或簇号为0，如果是，则直接返回，不执行清除操作。
        if self.is_dir() || first_cluster == 0 {
            return;
        }
        for i in 0..self.long_pos_vec.len() {
            // 将长目录项的内容清空
            self.modify_long_dirent(i, |long_entry: &mut LongDirEntry| {
                long_entry.clear();
            });
        }
        // 对短目录项执行修改操作，将其内容清空
        self.modify_short_dirent(|short_entry: &mut ShortDirEntry| {
            short_entry.clear();
        });
        let all_clusters = self
            .fs
            .read()
            .get_fat()
            .read()
            .get_all_cluster_of(first_cluster, self.block_device.clone());
        //self.fs.write().dealloc_cluster(all_clusters);
        let fs_reader = self.fs.read();
        // 释放文件所占用的所有簇
        fs_reader.dealloc_cluster(all_clusters);
        //fs_reader.cache_write_back();
    }

    /* 查找可用目录项，返回offset，簇不够也会返回相应的offset，caller需要及时分配 */
    fn find_free_dirent(&self) -> Option<usize> {
        if !self.is_dir() {
            return None;
        }
        let mut offset = 0;
        loop {
            let mut tmp_dirent = ShortDirEntry::new();
            let read_size = self.read_short_dirent(|short_entry: &ShortDirEntry| {
                short_entry.read_at(
                    offset,
                    tmp_dirent.as_bytes_mut(),
                    &self.fs,
                    &self.fs.read().get_fat(),
                    &self.block_device,
                )
            });
            if tmp_dirent.is_empty() || read_size == 0 {
                return Some(offset);
            }
            offset += DIRENT_SZ;
        }
    }

    /// 删除文件
    pub fn remove(&self) -> usize {
        let first_cluster: u32 = self.first_cluster();
        // 删除长文件名目录项
        for i in 0..self.long_pos_vec.len() {
            self.modify_long_dirent(i, |long_entry: &mut LongDirEntry| {
                long_entry.delete();
            });
        }
        // 删除短文件名目录项
        self.modify_short_dirent(|short_entry: &mut ShortDirEntry| {
            short_entry.delete();
        });
        // 回收对应簇
        let all_clusters = self
            .fs
            .read()
            .get_fat()
            .read()
            .get_all_cluster_of(first_cluster, self.block_device.clone());
        self.fs.write().dealloc_cluster(all_clusters.clone());
        return all_clusters.len();
    }

    /// 返回：(st_size, st_blksize, st_blocks, is_dir, time)
    /// todo：时间等
    pub fn stat(&self) -> (i64, i64, u64, bool, u64) {
        self.read_short_dirent(|short_entry: &ShortDirEntry| {
            let first_cluster = short_entry.first_cluster();
            let mut file_size = short_entry.file_size();
            let fs_reader = self.fs.read();
            let fat = fs_reader.get_fat();
            let fat_reader = fat.read();
            let cluster_num = fat_reader.count_claster_num(first_cluster, self.block_device.clone());
            let blocks = cluster_num * fs_reader.sectors_per_cluster();
            if self.is_dir() {
                // 如果当前文件是目录文件，则将文件大小file_size设置为簇数乘以每个簇的字节数，即目录文件的dir_file_size字段为0。
                file_size = cluster_num * fs_reader.bytes_per_cluster();
            }
            (file_size as i64, 512 as i64, blocks as u64, self.is_dir(), short_entry.time())
        })
    }

    pub fn set_time(&self, tv_sec: u64, tv_nsec: u64) {
        self.modify_short_dirent(|short_entry: &mut ShortDirEntry| {
            short_entry.set_time(tv_sec, tv_nsec);
        })
    }

    // 目前返回：(d_name, d_off, d_type)
    // 接受一个offset参数，表示目录项的偏移量。
    pub fn dirent_info(&self, offset: usize) -> Option<(String, u32, u32, u8)> {
        // 首先进行了一些判断，如果当前文件不是目录，则返回None。
        if !self.is_dir() {
            return None;
        }
        let mut file_entry = LongDirEntry::new();
        let mut offset = offset;
        let mut name = String::new();
        let mut is_long = false;
        // 使用一个循环来读取目录项的信息。
        // 在循环内部，首先创建了一个LongDirEntry实例file_entry，并初始化一些变量。
        //let mut order:u8 = 0;
        loop {
            let read_sz = self.read_short_dirent(|curr_ent: &ShortDirEntry| {
                curr_ent.read_at(
                    offset,
                    file_entry.as_bytes_mut(),
                    &self.fs,
                    &self.fs.read().get_fat(),
                    &self.block_device,
                )
            });
            // 如果读取的目录项的大小不等于DIRENT_SZ（目录项的大小）或者file_entry为空（表示没有有效的目录项），则返回None。
            if read_sz != DIRENT_SZ || file_entry.is_empty() {
                return None;
            }
            // 如果读取到的目录项是被删除的，则重启搜索，更新偏移量、清空名称、重置is_long标志。
            if file_entry.is_deleted() {
                //if meet delete ent, search should be restart
                offset += DIRENT_SZ;
                name.clear();
                is_long = false;
                continue;
            }
            // 如果读取到的目录项不是长目录项（attr() != ATTR_LONG_NAME），
            // 则表示读取到了一个短目录项，获取名称、属性、第一个簇号等信息，
            // 并将偏移量加上目录项的大小，然后返回这些信息
            // 将名称、偏移量、第一个簇号、属性封装成一个元组，并返回该元组。
            if file_entry.attr() != ATTR_LONG_NAME {
                let (_, se_array, _) = unsafe { file_entry.as_bytes_mut().align_to_mut::<ShortDirEntry>() };
                let short_ent = se_array[0];
                if !is_long {
                    name = short_ent.get_name_lowercase();
                }
                //println!("---{}", short_ent.get_name_lowercase());
                let attribute = short_ent.attr();
                let first_cluster = short_ent.first_cluster();
                offset += DIRENT_SZ;
                return Some((name, offset as u32, first_cluster, attribute));
            } else {
                // 如果读取到的目录项是长目录项（attr() == ATTR_LONG_NAME），则将is_long标志设置为true，并将长目录项的名称拼接到name变量中。
                is_long = true;
                //order += 1;
                name.insert_str(0, file_entry.get_name_format().as_str());
                //println!("--{}", long_ent.get_name_format().as_str());
            }
            // 更新偏移量，继续下一轮循环。
            offset += DIRENT_SZ;
        }
    }
}

/// 创建根目录的虚拟文件
pub fn create_root_vfile(fs_manager: &Arc<RwLock<FAT32Manager>>) -> VFile {
    let long_pos_vec: Vec<(usize, usize)> = Vec::new();
    VFile::new(
        String::from("/"),
        0,
        0,
        long_pos_vec,
        ATTR_DIRECTORY,
        Arc::clone(fs_manager),
        fs_manager.read().block_device(),
    )
}

