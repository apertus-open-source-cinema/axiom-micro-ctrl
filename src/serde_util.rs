use lazy_static::lazy_static;
use serde::{Deserialize, Deserializer};
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Mutex;

pub struct FileOpener {
    path: Mutex<Option<PathBuf>>,
}

impl FileOpener {
    pub fn set_path(&self, path: PathBuf) {
        *self.path.lock().unwrap() = Some(path);
    }

    pub fn open(&self, filename: &str) -> std::io::Result<File> {
        let path = match *self.path.lock().unwrap() {
            Some(ref path) => path.with_file_name(filename),
            None => PathBuf::from(filename),
        };

        File::open(path)
    }
}

lazy_static! {
    pub static ref file_opener: FileOpener = FileOpener {
        path: Mutex::new(None),
    };
}

pub fn by_path<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    for<'a> T: Deserialize<'a>,
    D: Deserializer<'de>,
{
    let path = String::deserialize(deserializer)?;

    let mut f = file_opener.open(&path).expect("file not found");

    let mut contents = String::new();
    f.read_to_string(&mut contents)
        .expect("something went wrong reading the file");

    Ok(serde_yaml::from_str(&contents).unwrap())
}

pub fn bool_false() -> bool {
    false
}
