use serde::Serialize;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub saved: usize,
    pub parts: Vec<Part>,
    pub code: Option<&'static str>,
}
#[derive(Debug, Serialize)]
pub struct Part {
    pub index: usize,
    pub status: &'static str,
}

trait Writer {
    type Temporary;
    fn stage(&mut self, content: &[u8]) -> io::Result<Self::Temporary>;
    fn publish(&mut self, temp: &Self::Temporary, index: usize) -> io::Result<()>;
    fn cleanup(&mut self, temp: Self::Temporary);
}
fn bundle<W: Writer>(contents: &[Vec<u8>], writer: &mut W) -> Outcome {
    let mut result = Outcome {
        saved: 0,
        parts: (0..contents.len())
            .map(|index| Part {
                index,
                status: "notWritten",
            })
            .collect(),
        code: None,
    };
    let mut staged = Vec::new();
    for (index, content) in contents.iter().enumerate() {
        match writer.stage(content) {
            Ok(temp) => staged.push(temp),
            Err(_) => {
                result.parts[index].status = "failed";
                result.code = Some("LEG-5002");
                break;
            }
        }
    }
    if result.code.is_none() {
        for (index, temp) in staged.iter().enumerate() {
            if writer.publish(temp, index).is_err() {
                result.parts[index].status = "failed";
                result.code = Some("LEG-5002");
                break;
            }
            result.parts[index].status = "saved";
            result.saved += 1;
        }
    }
    for temp in staged {
        writer.cleanup(temp);
    }
    result
}
struct Files {
    parent: PathBuf,
    targets: Vec<PathBuf>,
}
impl Writer for Files {
    type Temporary = PathBuf;
    fn stage(&mut self, content: &[u8]) -> io::Result<PathBuf> {
        use io::Write;
        let path = self.parent.join(format!(
            ".legacists-{}.tmp",
            crate::application::id("export")
        ));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        if let Err(error) = file.write_all(content).and_then(|()| file.sync_all()) {
            drop(file);
            let _ = fs::remove_file(&path);
            return Err(error);
        }
        Ok(path)
    }
    fn publish(&mut self, temp: &PathBuf, index: usize) -> io::Result<()> {
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            let from: Vec<_> = temp.as_os_str().encode_wide().chain(Some(0)).collect();
            let to: Vec<_> = self.targets[index]
                .as_os_str()
                .encode_wide()
                .chain(Some(0))
                .collect();
            // MOVEFILE_WRITE_THROUGH, deliberately without REPLACE_EXISTING.
            if unsafe {
                windows_sys::Win32::Storage::FileSystem::MoveFileExW(from.as_ptr(), to.as_ptr(), 8)
            } == 0
            {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }
        #[cfg(not(windows))]
        {
            fs::hard_link(temp, &self.targets[index])
        }
    }
    fn cleanup(&mut self, temp: PathBuf) {
        let _ = fs::remove_file(temp);
    }
}
pub fn save(parent: &Path, targets: Vec<PathBuf>, contents: &[Vec<u8>]) -> Outcome {
    bundle(
        contents,
        &mut Files {
            parent: parent.to_owned(),
            targets,
        },
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Faults {
        fail_stage: Option<usize>,
        fail_publish: Option<usize>,
        staged: usize,
        published: Vec<usize>,
        cleaned: Vec<usize>,
    }
    impl Writer for Faults {
        type Temporary = usize;
        fn stage(&mut self, _: &[u8]) -> io::Result<usize> {
            let i = self.staged;
            self.staged += 1;
            if self.fail_stage == Some(i) {
                Err(io::Error::other("synthetic"))
            } else {
                Ok(i)
            }
        }
        fn publish(&mut self, _: &usize, index: usize) -> io::Result<()> {
            if self.fail_publish == Some(index) {
                Err(io::Error::other("synthetic"))
            } else {
                self.published.push(index);
                Ok(())
            }
        }
        fn cleanup(&mut self, i: usize) {
            self.cleaned.push(i);
        }
    }
    #[test]
    fn t17_stage_failure_publishes_nothing_cleans_only_its_temps() {
        let mut w = Faults {
            fail_stage: Some(1),
            fail_publish: None,
            staged: 0,
            published: vec![],
            cleaned: vec![],
        };
        let result = bundle(&[vec![1], vec![2], vec![3]], &mut w);
        assert_eq!(result.saved, 0);
        assert_eq!(result.parts[1].status, "failed");
        assert_eq!(w.cleaned, vec![0]);
        assert!(w.published.is_empty());
    }
    #[test]
    fn t17_partial_publish_reports_each_part_and_preserves_published() {
        let mut w = Faults {
            fail_stage: None,
            fail_publish: Some(1),
            staged: 0,
            published: vec![],
            cleaned: vec![],
        };
        let result = bundle(&[vec![1], vec![2], vec![3]], &mut w);
        assert_eq!(result.saved, 1);
        assert_eq!(
            result.parts.iter().map(|p| p.status).collect::<Vec<_>>(),
            ["saved", "failed", "notWritten"]
        );
        assert_eq!(w.published, vec![0]);
        assert_eq!(w.cleaned, vec![0, 1, 2]);
    }
    #[test]
    fn t17_existing_destination_never_replaced() {
        let path = std::env::temp_dir().join(crate::application::id("legacists-output-test"));
        fs::create_dir(&path).unwrap();
        let target = path.join("existing.txt");
        fs::write(&target, b"original").unwrap();
        let outcome = save(&path, vec![target.clone()], &[b"new".to_vec()]);
        assert_eq!(outcome.saved, 0);
        assert_eq!(fs::read(target).unwrap(), b"original");
        assert_eq!(fs::read_dir(&path).unwrap().count(), 1);
        fs::remove_dir_all(path).unwrap();
    }
}
