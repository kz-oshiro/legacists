use legacists_core::{
    input::{has_link_component, relative_path},
    AnalysisOptions,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub type Result<T> = std::result::Result<T, &'static str>;
fn id() -> String {
    format!(
        "{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Settings {
    pub schema_version: u32,
    pub revision: u64,
    pub project: Option<PathBuf>,
    pub selected: Vec<String>,
    pub analysis: AnalysisOptions,
    pub max_bytes: usize,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: 1,
            revision: 0,
            project: None,
            selected: Vec::new(),
            analysis: AnalysisOptions::default(),
            max_bytes: 800000,
        }
    }
}
impl Settings {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != 1
            || self.project.as_ref().is_some_and(|p| {
                !p.is_absolute()
                    || p.components()
                        .any(|c| matches!(c, std::path::Component::ParentDir))
            })
            || !(16384..=1_000_000_000).contains(&self.max_bytes)
            || self.selected.iter().any(|p| !relative_path(p))
            || self.analysis.exclusions.iter().any(|p| !relative_path(p))
            || self.analysis.encodings.keys().any(|p| !relative_path(p))
            || self
                .analysis
                .web_root
                .as_ref()
                .is_some_and(|p| !p.is_empty() && !relative_path(p))
        {
            return Err("LEG-6001");
        }
        Ok(())
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Bootstrap {
    schema_version: u32,
    root: PathBuf,
    owner: String,
    pending: Option<Pending>,
    previous: Option<(PathBuf, String)>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum Pending {
    Migrate { root: PathBuf, owner: String },
    Reset,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Marker {
    app: String,
    owner: String,
    schema_version: u32,
}

pub struct Storage {
    pub default_root: PathBuf,
    pub root: PathBuf,
    pub bootstrap_path: PathBuf,
    pub settings: Settings,
    pub notice: Option<String>,
    pub pending: bool,
    bootstrap: Bootstrap,
    writable: bool,
    _lock: Lock,
}

struct Lock {
    path: PathBuf,
}
impl Drop for Lock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn process_creation(pid: u32) -> Option<u64> {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::{
            Foundation::{CloseHandle, FILETIME},
            System::Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
        };
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return None;
        }
        let mut creation = FILETIME::default();
        let mut exit = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        let ok = GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user);
        CloseHandle(handle);
        if ok == 0 || exit.dwHighDateTime != 0 || exit.dwLowDateTime != 0 {
            return None;
        }
        Some((creation.dwHighDateTime as u64) << 32 | creation.dwLowDateTime as u64)
    }
    #[cfg(not(windows))]
    {
        let _ = pid;
        None
    }
}

fn lock(path: &Path) -> Result<Lock> {
    if path.exists() {
        let old: serde_json::Value =
            serde_json::from_slice(&fs::read(path).map_err(|_| "LEG-6001")?)
                .map_err(|_| "LEG-6001")?;
        let pid = old["pid"].as_u64().ok_or("LEG-6001")? as u32;
        let creation = old["creation"].as_u64().ok_or("LEG-6001")?;
        if process_creation(pid) == Some(creation) {
            return Err("DATA_IN_USE");
        }
        fs::remove_file(path).map_err(|_| "LEG-6001")?;
    }
    let pid = std::process::id();
    let creation = process_creation(pid).ok_or("LEG-6001")?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| "DATA_IN_USE")?;
    file.write_all(
        serde_json::json!({"pid":pid,"creation":creation})
            .to_string()
            .as_bytes(),
    )
    .map_err(|_| "LEG-6001")?;
    file.sync_all().map_err(|_| "LEG-6001")?;
    Ok(Lock { path: path.into() })
}

pub fn overlap(a: &Path, b: &Path) -> bool {
    let a = normalized(a);
    let b = normalized(b);
    a.starts_with(&b) || b.starts_with(&a)
}

pub fn normalized(path: &Path) -> PathBuf {
    PathBuf::from(
        path.to_string_lossy()
            .trim_start_matches("\\\\?\\")
            .replace('/', "\\")
            .to_lowercase(),
    )
}

fn validate_root(root: &Path) -> Result<()> {
    if !root.is_absolute()
        || root.parent().is_none()
        || root
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        || has_link_component(root)
    {
        return Err("LEG-6001");
    }
    let name = root
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("LEG-6001")?;
    if !matches!(name, "data" | "legacists-data") {
        return Err("LEG-6001");
    }
    Ok(())
}

fn marker(root: &Path, owner: &str) -> Result<()> {
    validate_root(root)?;
    let path = root.join("owner.json");
    if has_link_component(&path) {
        return Err("LEG-6001");
    }
    let m: Marker =
        serde_json::from_slice(&fs::read(path).map_err(|_| "LEG-6001")?).map_err(|_| "LEG-6001")?;
    if m.app != "legacists" || m.owner != owner || m.schema_version != 1 {
        return Err("LEG-6001");
    }
    Ok(())
}

fn create_root(root: &Path, owner: &str) -> Result<()> {
    validate_root(root)?;
    if root.exists() && fs::read_dir(root).map_err(|_| "LEG-6001")?.next().is_some() {
        return marker(root, owner);
    }
    fs::create_dir_all(root).map_err(|_| "LEG-6001")?;
    atomic_json(
        &root.join("owner.json"),
        &Marker {
            app: "legacists".into(),
            owner: owner.into(),
            schema_version: 1,
        },
    )
}

pub fn atomic_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or("LEG-6001")?;
    if has_link_component(parent) || has_link_component(path) {
        return Err("LEG-6001");
    }
    let temporary = parent.join(format!(".legacists-{}.tmp", id()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|_| "LEG-6001")?;
        file.write_all(bytes).map_err(|_| "LEG-6001")?;
        file.sync_all().map_err(|_| "LEG-6001")?;
        drop(file);
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            use windows_sys::Win32::Storage::FileSystem::{
                MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
            };
            let from: Vec<_> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
            let to: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            if unsafe {
                MoveFileExW(
                    from.as_ptr(),
                    to.as_ptr(),
                    MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                )
            } == 0
            {
                return Err("LEG-6001");
            }
        }
        #[cfg(not(windows))]
        fs::rename(&temporary, path).map_err(|_| "LEG-6001")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}
fn atomic_json(path: &Path, value: &impl Serialize) -> Result<()> {
    atomic_bytes(
        path,
        &serde_json::to_vec_pretty(value).map_err(|_| "LEG-6001")?,
    )
}

fn remove_owned(root: &Path, owner: &str) -> Result<()> {
    marker(root, owner)?;
    let entries: Vec<_> = fs::read_dir(root)
        .map_err(|_| "LEG-6001")?
        .collect::<std::result::Result<_, _>>()
        .map_err(|_| "LEG-6001")?;
    if entries.iter().any(|e| {
        !matches!(
            e.file_name().to_str(),
            Some("owner.json" | "settings.json" | "runtime")
        ) || has_link_component(&e.path())
    }) {
        return Err("LEG-6001");
    }
    let runtime = root.join("runtime");
    if runtime.exists() {
        // Verify every descendant before a bounded recursive removal.
        let canonical = fs::canonicalize(root).map_err(|_| "LEG-6001")?;
        let mut stack = vec![runtime.clone()];
        while let Some(path) = stack.pop() {
            if has_link_component(&path)
                || !fs::canonicalize(&path)
                    .map_err(|_| "LEG-6001")?
                    .starts_with(&canonical)
            {
                return Err("LEG-6001");
            }
            if path.is_dir() {
                for e in fs::read_dir(path).map_err(|_| "LEG-6001")? {
                    stack.push(e.map_err(|_| "LEG-6001")?.path());
                }
            }
        }
        fs::remove_dir_all(&runtime).map_err(|_| "LEG-6001")?;
    }
    let settings = root.join("settings.json");
    if settings.exists() {
        fs::remove_file(settings).map_err(|_| "LEG-6001")?;
    }
    fs::remove_file(root.join("owner.json")).map_err(|_| "LEG-6001")?;
    fs::remove_dir(root).map_err(|_| "LEG-6001")?;
    Ok(())
}

impl Storage {
    pub fn open(local_appdata: &Path) -> Result<Self> {
        let base = local_appdata.join("legacists");
        if has_link_component(&base) {
            return Err("LEG-6001");
        }
        fs::create_dir_all(&base).map_err(|_| "LEG-6001")?;
        let guard = lock(&base.join("session.lock"))?;
        let default_root = base.join("data");
        let bootstrap_path = base.join("bootstrap.json");
        let fresh = || Bootstrap {
            schema_version: 1,
            root: default_root.clone(),
            owner: id(),
            pending: None,
            previous: None,
        };
        let mut bootstrap = if bootstrap_path.exists() {
            serde_json::from_slice::<Bootstrap>(&fs::read(&bootstrap_path).map_err(|_| "LEG-6001")?)
                .map_err(|_| "LEG-6001")?
        } else {
            fresh()
        };
        if bootstrap.schema_version != 1 {
            return Err("LEG-6001");
        }
        let mut notice = None;
        match bootstrap.pending.clone() {
            Some(Pending::Reset) => {
                if remove_owned(&bootstrap.root, &bootstrap.owner).is_ok() {
                    bootstrap = fresh();
                    atomic_json(&bootstrap_path, &bootstrap)?;
                } else {
                    notice=Some("LEG-6001: 保存データ削除は未完了です。権限・ロック・所有範囲を確認してください。".into());
                }
            }
            Some(Pending::Migrate { root, owner }) => {
                if marker(&root, &owner).is_ok() {
                    let next: std::result::Result<Settings, _> =
                        fs::read(root.join("settings.json"))
                            .map_err(|_| ())
                            .and_then(|b| serde_json::from_slice(&b).map_err(|_| ()));
                    if next.as_ref().is_ok_and(|s| s.validate().is_ok()) {
                        let old = (bootstrap.root.clone(), bootstrap.owner.clone());
                        bootstrap.root = root;
                        bootstrap.owner = owner;
                        bootstrap.pending = None;
                        bootstrap.previous = Some(old);
                        atomic_json(&bootstrap_path, &bootstrap)?;
                    } else {
                        bootstrap.pending = None;
                        atomic_json(&bootstrap_path, &bootstrap)?;
                        notice = Some("LEG-6001: 移行に失敗し、旧保存先へ戻しました。".into());
                    }
                } else {
                    bootstrap.pending = None;
                    atomic_json(&bootstrap_path, &bootstrap)?;
                    notice = Some("LEG-6001: 移行先を検証できず、旧保存先へ戻しました。".into());
                }
            }
            None => {}
        }
        if let Some((root, owner)) = &bootstrap.previous {
            if !overlap(root, &bootstrap.root) && remove_owned(root, owner).is_ok() {
                bootstrap.previous = None;
                atomic_json(&bootstrap_path, &bootstrap)?;
            } else {
                notice = Some("LEG-6001: 旧保存領域の片付けが未完了です。".into());
            }
        }
        create_root(&bootstrap.root, &bootstrap.owner)?;
        if !bootstrap_path.exists() {
            atomic_json(&bootstrap_path, &bootstrap)?;
        }
        let settings_path = bootstrap.root.join("settings.json");
        let mut writable = true;
        let settings = if settings_path.exists() {
            match fs::read(&settings_path)
                .ok()
                .and_then(|b| serde_json::from_slice::<Settings>(&b).ok())
                .filter(|s| s.validate().is_ok())
            {
                Some(s) => s,
                None => {
                    writable = false;
                    notice=Some("LEG-6001: 設定が破損しているか新しい形式です。元の設定を保護し、既定値で起動しました。".into());
                    Settings::default()
                }
            }
        } else {
            Settings::default()
        };
        Ok(Self {
            default_root,
            root: bootstrap.root.clone(),
            bootstrap_path,
            settings,
            notice,
            pending: bootstrap.pending.is_some(),
            bootstrap,
            writable,
            _lock: guard,
        })
    }
    pub fn update(&mut self, mut settings: Settings, revision: u64) -> Result<()> {
        if revision != self.settings.revision {
            return Err("SETTINGS_CONFLICT");
        }
        if !self.writable || self.pending {
            return Err("LEG-6001");
        }
        settings.validate()?;
        marker(&self.root, &self.bootstrap.owner)?;
        settings.revision = revision + 1;
        atomic_json(&self.root.join("settings.json"), &settings)?;
        self.settings = settings;
        Ok(())
    }
    pub fn migrate(&mut self, parent: &Path) -> Result<()> {
        if self.pending || !self.writable {
            return Err("LEG-6001");
        }
        let parent = fs::canonicalize(parent).map_err(|_| "LEG-6001")?;
        let root = parent.join("legacists-data");
        if has_link_component(parent.as_path())
            || overlap(&root, &self.root)
            || self
                .settings
                .project
                .as_ref()
                .is_some_and(|p| overlap(&root, p))
        {
            return Err("LEG-6001");
        }
        let owner = id();
        create_root(&root, &owner)?;
        atomic_json(&root.join("settings.json"), &self.settings)?;
        self.bootstrap.pending = Some(Pending::Migrate { root, owner });
        atomic_json(&self.bootstrap_path, &self.bootstrap)?;
        self.pending = true;
        Ok(())
    }
    pub fn reset(&mut self) -> Result<()> {
        marker(&self.root, &self.bootstrap.owner)?;
        self.bootstrap.pending = Some(Pending::Reset);
        atomic_json(&self.bootstrap_path, &self.bootstrap)?;
        self.pending = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn area() -> PathBuf {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../outputs/storage-tests")
            .join(id());
        fs::create_dir_all(&p).unwrap();
        fs::canonicalize(p).unwrap()
    }
    #[test]
    fn t15_settings_roundtrip_conflict_and_no_source_persistence() {
        let area = area();
        let mut s = Storage::open(&area).unwrap();
        let mut next = s.settings.clone();
        next.selected = vec!["web/index.jsp".into()];
        next.analysis
            .encodings
            .insert("web/index.jsp".into(), legacists_core::Encoding::Sjis);
        next.analysis.exclusions.push("ignored".into());
        s.update(next.clone(), 0).unwrap();
        assert_eq!(s.update(next, 0).unwrap_err(), "SETTINGS_CONFLICT");
        let root = s.root.clone();
        drop(s);
        let s = Storage::open(&area).unwrap();
        assert_eq!(s.settings.selected, vec!["web/index.jsp"]);
        assert_eq!(s.settings.revision, 1);
        assert_eq!(fs::read_dir(root).unwrap().count(), 2);
    }
    #[test]
    fn t15_corrupt_or_future_settings_are_preserved() {
        for bytes in [b"{broken".as_slice(), b"{\"schemaVersion\":99}".as_slice()] {
            let area = area();
            let s = Storage::open(&area).unwrap();
            let path = s.root.join("settings.json");
            drop(s);
            fs::write(&path, bytes).unwrap();
            let mut s = Storage::open(&area).unwrap();
            assert!(s.notice.is_some());
            assert!(s.update(Settings::default(), 0).is_err());
            assert_eq!(fs::read(path).unwrap(), bytes);
        }
    }
    #[test]
    fn t15_migration_copies_settings_not_runtime_and_preserves_parent() {
        let area = area();
        let target = area.join("target");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("unrelated.txt"), "unrelated").unwrap();
        let mut s = Storage::open(&area).unwrap();
        let old = s.root.clone();
        fs::create_dir(old.join("runtime")).unwrap();
        fs::write(old.join("runtime/old-cache"), "not copied").unwrap();
        let mut next = s.settings.clone();
        next.selected = vec!["a.java".into()];
        s.update(next, 0).unwrap();
        s.migrate(&target).unwrap();
        assert!(s.pending);
        assert!(old.exists());
        drop(s);
        let s = Storage::open(&area).unwrap();
        assert!(s.root.ends_with("legacists-data"));
        assert_eq!(s.settings.selected, vec!["a.java"]);
        assert!(!s.root.join("runtime").exists());
        assert!(!old.exists());
        assert_eq!(
            fs::read_to_string(target.join("unrelated.txt")).unwrap(),
            "unrelated"
        );
    }
    #[test]
    fn t15_migration_invalid_copy_rolls_back() {
        let area = area();
        let target = area.join("target");
        fs::create_dir(&target).unwrap();
        let mut s = Storage::open(&area).unwrap();
        let old = s.root.clone();
        s.migrate(&target).unwrap();
        fs::write(target.join("legacists-data/settings.json"), "invalid").unwrap();
        drop(s);
        let s = Storage::open(&area).unwrap();
        assert_eq!(s.root, old);
        assert!(s.notice.is_some());
        assert!(!s.pending);
    }
    #[test]
    fn t16_reset_removes_only_owned_data_on_restart() {
        let area = area();
        let project = area.join("project");
        fs::create_dir(&project).unwrap();
        fs::write(project.join("source.java"), "class Original {}").unwrap();
        fs::write(area.join("saved.txt"), "exported").unwrap();
        let mut s = Storage::open(&area).unwrap();
        let mut settings = s.settings.clone();
        settings.project = Some(project.clone());
        settings.selected = vec!["source.java".into()];
        s.update(settings, 0).unwrap();
        s.reset().unwrap();
        assert!(s.root.join("settings.json").exists());
        drop(s);
        let s = Storage::open(&area).unwrap();
        assert!(s.settings.selected.is_empty());
        assert!(s.settings.project.is_none());
        assert_eq!(
            fs::read_to_string(project.join("source.java")).unwrap(),
            "class Original {}"
        );
        assert_eq!(
            fs::read_to_string(area.join("saved.txt")).unwrap(),
            "exported"
        );
    }
    #[test]
    fn t16_unknown_file_and_owner_mismatch_are_not_deleted() {
        let area = area();
        let mut s = Storage::open(&area).unwrap();
        let root = s.root.clone();
        fs::write(root.join("unrelated.txt"), "keep").unwrap();
        s.reset().unwrap();
        drop(s);
        let s = Storage::open(&area).unwrap();
        assert!(s.pending);
        assert!(s.notice.is_some());
        assert_eq!(
            fs::read_to_string(root.join("unrelated.txt")).unwrap(),
            "keep"
        );
        assert!(remove_owned(&root, "wrong-owner").is_err());
        assert!(root.join("owner.json").exists());
    }
    #[test]
    fn t16_live_instance_lock_and_project_overlap_rejected() {
        let area = area();
        let mut s = Storage::open(&area).unwrap();
        assert!(matches!(Storage::open(&area), Err("DATA_IN_USE")));
        let project = area.join("project");
        fs::create_dir(&project).unwrap();
        s.settings.project = Some(project.clone());
        assert!(s.migrate(&project).is_err());
        assert!(overlap(&area, &s.root));
    }
    #[cfg(windows)]
    #[test]
    fn t15_locked_atomic_write_preserves_previous_settings() {
        use std::os::windows::fs::OpenOptionsExt;
        let area = area();
        let mut s = Storage::open(&area).unwrap();
        s.update(s.settings.clone(), 0).unwrap();
        let path = s.root.join("settings.json");
        let bytes = fs::read(&path).unwrap();
        let _locked = OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&path)
            .unwrap();
        assert!(s.update(s.settings.clone(), 1).is_err());
        assert_eq!(s.settings.revision, 1);
        drop(_locked);
        assert_eq!(fs::read(path).unwrap(), bytes);
        assert!(!fs::read_dir(&s.root).unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp")));
    }
}
