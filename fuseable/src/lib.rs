extern crate time;
extern crate fuse_mt;
extern crate lru;
extern crate itertools;

use fuse_mt::*;
use std::path::Path;
use time::*;
use std::sync::RwLock;
use lru::LruCache;
use std::ffi::OsString;
use std::collections::{BTreeMap, HashMap};
use std::ops::{Deref, DerefMut};
use itertools::Itertools;

#[derive(Debug, Clone)]
pub enum Either<A, B> {
    Left(A),
    Right(B),
}

pub trait Fuseable: Sync + Send {
    fn is_dir(&self, path: &mut Iterator<Item = &str>) -> Result<bool, ()>;
    fn read(&self, path: &mut Iterator<Item = &str>) -> Result<Either<Vec<String>, String>, ()>;
    fn write(&mut self, path: &mut Iterator<Item = &str>, value: Vec<u8>) -> Result<(), ()>;
}

macro_rules! impl_fuseable_with_to_string {
    ($t:ident) => {
        impl Fuseable for $t {
            fn is_dir(&self, path: &mut Iterator<Item = &str>) -> Result<bool, ()> {
                match path.next() {
                    Some(_) => Err(()),
                    None => Ok(false),
                }
            }

            fn read(
                &self,
                path: &mut Iterator<Item = &str>,
            ) -> Result<Either<Vec<String>, String>, ()> {
                match path.next() {
                    Some(_) => Err(()),
                    None => Ok(Either::Right(self.to_string())),
                }
            }

            fn write(
                &mut self,
                path: &mut Iterator<Item = &str>,
                value: Vec<u8>,
            ) -> Result<(), ()> {
                Err(())
            }
        }
    };
}

impl_fuseable_with_to_string!(String);
impl_fuseable_with_to_string!(u8);
impl_fuseable_with_to_string!(i8);
impl_fuseable_with_to_string!(u16);
impl_fuseable_with_to_string!(i16);
impl_fuseable_with_to_string!(u32);
impl_fuseable_with_to_string!(i32);
impl_fuseable_with_to_string!(u64);
impl_fuseable_with_to_string!(i64);
impl_fuseable_with_to_string!(f32);
impl_fuseable_with_to_string!(f64);


impl<'a> Fuseable for &'a str {
    fn is_dir(&self, path: &mut Iterator<Item = &str>) -> Result<bool, ()> {
        match path.next() {
            Some(_) => Err(()),
            None => Ok(false),
        }
    }

    fn read(
        &self,
        path: &mut Iterator<Item = &str>,
    ) -> Result<Either<Vec<String>, String>, ()> {
        match path.next() {
            Some(_) => Err(()),
            None => Ok(Either::Right(self.to_string())),
        }
    }

    fn write(
        &mut self,
        path: &mut Iterator<Item = &str>,
        value: Vec<u8>,
    ) -> Result<(), ()> {
        Err(())
    }
}

impl<TY: Fuseable> Fuseable for Option<TY> {
    fn is_dir(&self, path: &mut Iterator<Item = &str>) -> Result<bool, ()> {
        match self {
            Some(v) => Fuseable::is_dir(v, path),
            None => Ok(false),
        }
    }

    fn read(&self, path: &mut Iterator<Item = &str>) -> Result<Either<Vec<String>, String>, ()> {
        match self {
            Some(v) => Fuseable::read(v, path),
            None => Ok(Either::Right("None".to_string())),
        }
    }

    fn write(&mut self, path: &mut Iterator<Item = &str>, value: Vec<u8>) -> Result<(), ()> {
        Err(())
    }
}

impl<'a, VT: Fuseable> Fuseable for BTreeMap<String, VT> {
    fn is_dir(&self, path: &mut Iterator<Item = &str>) -> Result<bool, ()> {
        match path.next() {
            Some(name) => match self.get(&name.to_string()) {
                Some(inner) => inner.is_dir(path),
                None => Err(()),
            },
            None => {
                Ok(true)
            }
        }
    }

    fn read(&self, path: &mut Iterator<Item = &str>) -> Result<Either<Vec<String>, String>, ()> {
        match path.next() {
            Some(name) => match self.get(&name.to_string()) {
                Some(inner) => inner.read(path),
                None => Err(()),
            },
            None => {
                let keys: Vec<_> = self.keys().cloned().collect();
                let keys = keys.into_iter().map(|k| String::from(k)).collect();
                Ok(Either::Left(keys))
            }
        }
    }

    fn write(&mut self, path: &mut Iterator<Item = &str>, value: Vec<u8>) -> Result<(), ()> {
        Err(())
    }
}

impl<'a, VT: Fuseable> Fuseable for HashMap<String, VT> {
    fn is_dir(&self, path: &mut Iterator<Item = &str>) -> Result<bool, ()> {
        match path.next() {
            Some(name) => match self.get(&name.to_string()) {
                Some(inner) => inner.is_dir(path),
                None => Err(()),
            },
            None => {
                Ok(true)
            }
        }
    }

    fn read(&self, path: &mut Iterator<Item = &str>) -> Result<Either<Vec<String>, String>, ()> {
        match path.next() {
            Some(name) => match self.get(&name.to_string()) {
                Some(inner) => inner.read(path),
                None => Err(()),
            },
            None => {
                let keys: Vec<_> = self.keys().cloned().collect();
                let keys = keys.into_iter().map(|k| String::from(k)).collect();
                Ok(Either::Left(keys))
            }
        }
    }

    fn write(&mut self, path: &mut Iterator<Item = &str>, value: Vec<u8>) -> Result<(), ()> {
        Err(())
    }
}

// The idea behind this is, that dir entries are always static, but file contents are not
pub struct CachedFuseable {
    is_dir_cache: RwLock<LruCache<String, Result<bool, ()>>>,
    read_dir_cache: RwLock<LruCache<String, Result<Either<Vec<String>, String>, ()>>>,
    fuseable: Box<Fuseable>
}

impl CachedFuseable {
    pub fn new(fuseable: Box<Fuseable>, cache_size: usize) -> CachedFuseable {
        CachedFuseable {
            is_dir_cache: RwLock::new(LruCache::new(cache_size)),
            read_dir_cache: RwLock::new(LruCache::new(cache_size)),
            fuseable: fuseable
        }
    }
}

impl Fuseable for CachedFuseable {
    fn is_dir(&self, path: &mut Iterator<Item = &str>) -> Result<bool, ()> {
        /*
        let (mut path, path_for_string) = path.tee();
        let path_string = path_for_string.collect::<Vec<_>>().concat();

        let mut update_cache = false;
        let is_dir = match self.is_dir_cache.write().unwrap().get(&path_string) {
            Some(r) => r.clone(),
            None => {
                update_cache = true;
                Fuseable::is_dir(self.fuseable.deref(), &mut path)
            }
        };

        if update_cache {
            self.is_dir_cache.write().unwrap().put(path_string, is_dir);
        }

        is_dir
        */

        Fuseable::is_dir(self.fuseable.deref(), path)
    }

    fn read(&self, path: &mut Iterator<Item = &str>) -> Result<Either<Vec<String>, String>, ()> {
        let (mut path, path_for_string) = path.tee();
        let path_string = path_for_string.collect::<Vec<_>>().concat();

        let mut update_cache = false;
        let read = match self.read_dir_cache.write().unwrap().get(&path_string) {
            Some(r) => {
                r.clone()
            },
            None => {
                update_cache = true;
                Fuseable::read(self.fuseable.deref(), &mut path)
            }
        };

        if update_cache {
            match read {
                Ok(Either::Left(_)) => {
                    self.read_dir_cache.write().unwrap().put(path_string, read.clone());
                },
                _ => {}
            }
        }

        read

        // Fuseable::read(self.fuseable.deref(), path)
    }

    fn write(&mut self, path: &mut Iterator<Item = &str>, value: Vec<u8>) -> Result<(), ()> {
        Fuseable::write(self.fuseable.deref_mut(), path, value)
    }
}


impl FilesystemMT for Box<Fuseable> {
    fn init(&self, _req: RequestInfo) -> ResultEmpty {
        Ok(())
    }

    fn destroy(&self, _req: RequestInfo) {}

    fn getattr(&self, _req: RequestInfo, path: &Path, fh: Option<u64>) -> ResultEntry {
        // println!("getattr: {:?}", path);
        if let Some(fh) = fh {
            println!("getattr: unhandled open file {}", fh);
            Err(1)
        } else {
            //            fn read(&self, path: &mut Iterator<Item = String>) -> Result<Either<Vec<String>, String>, ()>;

            Fuseable::is_dir(self.deref(), &mut path.to_string_lossy().split_terminator('/').skip(1))
                .map(|v| {
                    (
                        Timespec { sec: 0, nsec: 0 },
                        FileAttr {
                            size: 4096, // TODO(robin): this is shitty, but needed to convince the vfs to actually use the results of a read
                            blocks: 0,
                            atime: Timespec { sec: 0, nsec: 0 },
                            mtime: Timespec { sec: 0, nsec: 0 },
                            ctime: Timespec { sec: 0, nsec: 0 },
                            crtime: Timespec { sec: 0, nsec: 0 },
                            kind: match v {
                                true => FileType::Directory,
                                false => FileType::RegularFile,
                            },
                            perm: 0o777,
                            nlink: 2,
                            uid: 0,
                            gid: 0,
                            rdev: 0,
                            flags: 0,
                        },
                    )
                }).map_err(|_| 0)
        }
    }

    fn opendir(&self, _req: RequestInfo, path: &Path, _flags: u32) -> ResultOpen {
        // println!("opendir: {:?} (flags = {:#o})", path, _flags);

        match Fuseable::is_dir(self.deref(), &mut path.to_string_lossy().split_terminator('/').skip(1)) {
            Ok(true) => Ok((0, 0)),
            Ok(false) => Err(1),
            Err(_) => Err(1)
        }
    }

    fn readdir(&self, _req: RequestInfo, path: &Path, fh: u64) -> ResultReaddir {
        // println!("readdir: {:?}", path);
        Fuseable::read(self.deref(), &mut path.to_string_lossy().split_terminator('/').skip(1)).map(
            |v| {
                match v {
                    Either::Left(fields) => {
                        fields.iter().map(|f| {
                            DirectoryEntry {
                                name: OsString::from(f),
                                kind: FileType::Directory
                            }
                        }).collect()
                    }
                    _ => unimplemented!()
                }
            }).map_err(|_| 1)
    }
    fn open(&self, _req: RequestInfo, path: &Path, flags: u32) -> ResultOpen {
        // println!("open: {:?} flags={:#x}", path, flags);

        match Fuseable::is_dir(self.deref(), &mut path.to_string_lossy().split_terminator('/').skip(1)) {
            Ok(false) => Ok((0, 0)),
            Ok(true) => Err(1),
            Err(_) => Err(1)
        }
    }

    fn read(&self, _req: RequestInfo, path: &Path, fh: u64, offset: u64, size: u32) -> ResultData {
        // println!("read: {:?} {:#x} @ {:#x}", path, size, offset);

        match Fuseable::read(self.deref(), &mut path.to_string_lossy().split_terminator('/').skip(1)) {
            Ok(Either::Left(_)) => Err(1),
            Ok(Either::Right(s)) => Ok(s.into_bytes()),
            Err(_) => Err(1)
        }
    }

/*
    fn write(&self, _req: RequestInfo, path: &Path, fh: u64, offset: u64, data: Vec<u8>, _flags: u32) -> ResultWrite {
        println!("write: {:?} {:#x} @ {:#x}", path, data.len(), offset);

        match self.resolve_path(path) {
            Some(SensorFS::Reg(reg)) => {
                Ok(self.write_reg(reg, data) as u32)
            }

            _ => Err(1)
        }
    }

    fn truncate(&self, _req: RequestInfo, path: &Path, fh: Option<u64>, size: u64) -> ResultEmpty {
        println!("truncate: {:?} to {:#x}", path, size);
        Ok(())
    }
    */

    /*
    fn release(&self, _req: RequestInfo, path: &Path, fh: u64, _flags: u32, _lock_owner: u64, _flush: bool) -> ResultEmpty {
        println!("release: {:?}", path);
        Ok(())
    }

    fn flush(&self, _req: RequestInfo, path: &Path, fh: u64, _lock_owner: u64) -> ResultEmpty {
        println!("flush: {:?}", path);
        Ok(())
    }

    fn fsync(&self, _req: RequestInfo, path: &Path, fh: u64, datasync: bool) -> ResultEmpty {
        println!("fsync: {:?}, data={:?}", path, datasync);
        Err(1)
    }

    fn chmod(&self, _req: RequestInfo, path: &Path, fh: Option<u64>, mode: u32) -> ResultEmpty {
        println!("chown: {:?} to {:#o}", path, mode);
        Err(1)
    }

    fn chown(&self, _req: RequestInfo, path: &Path, fh: Option<u64>, uid: Option<u32>, gid: Option<u32>) -> ResultEmpty {
        println!("chmod: {:?} to {}:{}", path, uid.unwrap_or(::std::u32::MAX), gid.unwrap_or(::std::u32::MAX));
        Err(1)
    }

    fn utimens(&self, _req: RequestInfo, path: &Path, fh: Option<u64>, atime: Option<Timespec>, mtime: Option<Timespec>) -> ResultEmpty {
        println!("utimens: {:?}: {:?}, {:?}", path, atime, mtime);
        Err(1)
    }

    fn readlink(&self, _req: RequestInfo, path: &Path) -> ResultData {
        println!("readlink: {:?}", path);
        Err(1)
    }

    fn statfs(&self, _req: RequestInfo, path: &Path) -> ResultStatfs {
        println!("statfs: {:?}", path);
        Err(1)
    }

    fn fsyncdir(&self, _req: RequestInfo, path: &Path, fh: u64, datasync: bool) -> ResultEmpty {
        println!("fsyncdir: {:?} (datasync = {:?})", path, datasync);
        Err(1)
    }

    fn mknod(&self, _req: RequestInfo, parent_path: &Path, name: &OsStr, mode: u32, rdev: u32) -> ResultEntry {
        println!("mknod: {:?}/{:?} (mode={:#o}, rdev={})", parent_path, name, mode, rdev);
        Err(1)
    }

    fn mkdir(&self, _req: RequestInfo, parent_path: &Path, name: &OsStr, mode: u32) -> ResultEntry {
        println!("mkdir {:?}/{:?} (mode={:#o})", parent_path, name, mode);
        Err(1)
    }

    fn unlink(&self, _req: RequestInfo, parent_path: &Path, name: &OsStr) -> ResultEmpty {
        println!("unlink {:?}/{:?}", parent_path, name);
        Err(1)
    }

    fn rmdir(&self, _req: RequestInfo, parent_path: &Path, name: &OsStr) -> ResultEmpty {
        println!("rmdir: {:?}/{:?}", parent_path, name);
        Err(1)
    }

    fn symlink(&self, _req: RequestInfo, parent_path: &Path, name: &OsStr, target: &Path) -> ResultEntry {
        println!("symlink: {:?}/{:?} -> {:?}", parent_path, name, target);
        Err(1)
    }

    fn rename(&self, _req: RequestInfo, parent_path: &Path, name: &OsStr, newparent_path: &Path, newname: &OsStr) -> ResultEmpty {
        println!("rename: {:?}/{:?} -> {:?}/{:?}", parent_path, name, newparent_path, newname);
        Err(1)
    }

    fn link(&self, _req: RequestInfo, path: &Path, newparent: &Path, newname: &OsStr) -> ResultEntry {
        println!("link: {:?} -> {:?}/{:?}", path, newparent, newname);
        Err(1)
    }

    fn create(&self, _req: RequestInfo, parent: &Path, name: &OsStr, mode: u32, flags: u32) -> ResultCreate {
        println!("create: {:?}/{:?} (mode={:#o}, flags={:#x})", parent, name, mode, flags);
        Err(1)
    }

    /*
    fn listxattr(&self, _req: RequestInfo, path: &Path, size: u32) -> ResultXattr {
        println!("listxattr: {:?}", path);
        Err(1)
    }
    */

    fn getxattr(&self, _req: RequestInfo, path: &Path, name: &OsStr, size: u32) -> ResultXattr {
        println!("getxattr: {:?} {:?} {}", path, name, size);
        Err(1)
    }

    fn setxattr(&self, _req: RequestInfo, path: &Path, name: &OsStr, value: &[u8], flags: u32, position: u32) -> ResultEmpty {
        println!("setxattr: {:?} {:?} {} bytes, flags = {:#x}, pos = {}", path, name, value.len(), flags, position);
        Err(1)
    }

    fn removexattr(&self, _req: RequestInfo, path: &Path, name: &OsStr) -> ResultEmpty {
        println!("removexattr: {:?} {:?}", path, name);
        Err(1)
    }
    */
}
