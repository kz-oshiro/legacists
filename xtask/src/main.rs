use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    env, fs, io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

fn timestamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
}
fn capture(program: &str, args: &[&str], root: &Path) -> String {
    Command::new(program)
        .args(args)
        .current_dir(root)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}
fn fingerprint(root: &Path) -> io::Result<String> {
    let output = Command::new("git")
        .args([
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ])
        .current_dir(root)
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other("source inventory failed"));
    }
    let files = String::from_utf8(output.stdout).map_err(io::Error::other)?;
    let mut files: Vec<_> = files.split('\0').filter(|p| !p.is_empty()).collect();
    files.sort();
    let mut hasher = Sha256::new();
    for path in files {
        hasher.update(path.as_bytes());
        hasher.update([0]);
        hasher.update(fs::read(root.join(path))?);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

struct Run {
    root: PathBuf,
    directory: PathBuf,
    id: String,
    node: PathBuf,
    npm: PathBuf,
    report: Value,
}
impl Run {
    fn new(mode: &str) -> io::Result<Self> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_path_buf();
        let id = format!("{}-{}-{mode}", timestamp(), std::process::id());
        let directory = root.join("outputs/runs").join(&id);
        fs::create_dir_all(directory.join("logs"))?;
        let node_root = root.join("outputs/tools/node-v24.21.0-win-x64");
        let node = if node_root.join("node.exe").exists() {
            node_root.join("node.exe")
        } else {
            PathBuf::from("node.exe")
        };
        let npm = node_root.join("node_modules/npm/bin/npm-cli.js");
        let commit = capture("git", &["rev-parse", "HEAD"], &root);
        let status = capture("git", &["status", "--porcelain=v1"], &root);
        let source_fingerprint = fingerprint(&root)?;
        let environment=capture("pwsh",&["-NoProfile","-Command","$o=Get-CimInstance Win32_OperatingSystem;$c=Get-CimInstance Win32_Processor|Select-Object -First 1;@{os=$o.Caption;build=$o.BuildNumber;architecture=$o.OSArchitecture;cpu=$c.Name;ramBytes=[uint64]$o.TotalVisibleMemorySize*1024;webView2=(Get-ChildItem 'C:\\Program Files (x86)\\Microsoft\\EdgeWebView\\Application' -Directory|Where-Object Name -match '^\\d'|Select-Object -ExpandProperty Name);power=(powercfg /getactivescheme)}|ConvertTo-Json -Compress"],&root);
        let acceptance_exceptions = if matches!(mode, "dev" | "ci") {
            json!([{"name":"real-exe-integration","status":"not_required","testIds":["T-19 (real-EXE portion)","T-22"],"reason":"user-waived-initial-release-2026-10-12","issue":3}])
        } else {
            json!([])
        };
        let report = json!({"schemaVersion":1,"runId":id,"mode":mode,"scope":if mode=="dev"{"issue-3-development"}else{"selected-or-full-validation"},"source":{"commit":commit,"dirty":!status.is_empty(),"status":status,"fingerprint":source_fingerprint},"startedAtUnixMs":timestamp(),"environment":{"host":serde_json::from_str::<Value>(&environment).unwrap_or(json!({"unavailable":true})),"rust":capture("rustc",&["--version"],&root),"node":capture(node.to_str().unwrap(),&["--version"],&root)},"stages":[],"acceptanceExceptions":acceptance_exceptions,"artifacts":[],"success":false});
        Ok(Self {
            root,
            directory,
            id,
            node,
            npm,
            report,
        })
    }
    fn step(&mut self, name: &str, program: &Path, args: Vec<String>, cwd: Option<&Path>) -> bool {
        let started = timestamp();
        let timer = Instant::now();
        let logfile = self.directory.join("logs").join(format!("{name}.log"));
        let file = fs::File::create(&logfile).unwrap();
        let error = file.try_clone().unwrap();
        let mut command = Command::new(program);
        command
            .args(&args)
            .current_dir(cwd.unwrap_or(&self.root))
            .stdout(Stdio::from(file))
            .stderr(Stdio::from(error))
            .env("LEGACISTS_RUN_DIR", &self.directory)
            .env(
                "LEGACISTS_EXE",
                self.directory.join("artifacts/legacists-desktop.exe"),
            );
        if let Some(parent) = self.node.parent().filter(|p| !p.as_os_str().is_empty()) {
            let path = env::var_os("PATH").unwrap_or_default();
            let mut paths = vec![parent.to_path_buf()];
            paths.extend(env::split_paths(&path));
            command.env("PATH", env::join_paths(paths).unwrap());
        }
        println!("{name} ...");
        let result = command.status();
        let ok = result.as_ref().is_ok_and(|s| s.success());
        let reason = result.as_ref().err().map(ToString::to_string).or_else(|| {
            if ok {
                None
            } else {
                Some(format!(
                    "exit {:?}",
                    result.as_ref().ok().and_then(|s| s.code())
                ))
            }
        });
        self.report["stages"].as_array_mut().unwrap().push(json!({"name":name,"command":{"program":program,"args":args,"cwd":cwd.unwrap_or(&self.root)},"startedAtUnixMs":started,"endedAtUnixMs":timestamp(),"durationMs":timer.elapsed().as_millis(),"status":if ok{"passed"}else{"failed"},"reason":reason,"log":format!("logs/{name}.log")}));
        self.save().unwrap();
        if !ok {
            eprintln!("{name} failed: {}", logfile.display());
        }
        ok
    }
    fn cargo(&mut self, name: &str, args: &[&str]) -> bool {
        self.step(
            name,
            Path::new("cargo"),
            args.iter().map(|s| s.to_string()).collect(),
            None,
        )
    }
    fn node(&mut self, name: &str, args: &[&str]) -> bool {
        let node = self.node.clone();
        self.step(
            name,
            &node,
            args.iter().map(|s| s.to_string()).collect(),
            None,
        )
    }
    fn npm(&mut self, name: &str, args: &[&str]) -> bool {
        let node = self.node.clone();
        let mut command = vec![
            self.npm.to_string_lossy().to_string(),
            "--prefix".into(),
            "apps/desktop".into(),
        ];
        command.extend(args.iter().map(|s| s.to_string()));
        self.step(name, &node, command, None)
    }
    fn powershell(&mut self, name: &str, script: &str) -> bool {
        self.step(
            name,
            Path::new("pwsh"),
            vec!["-NoProfile".into(), "-File".into(), script.into()],
            None,
        )
    }
    fn save(&self) -> io::Result<()> {
        fs::write(
            self.directory.join("report.json"),
            serde_json::to_vec_pretty(&self.report)?,
        )?;
        let mut md=format!("# legacists validation\n\nRun: `{}`\nScope: `{}`\nSource: `{}`; dirty: `{}`\n\n| Stage | Result | Duration ms | Log |\n| --- | --- | --- | --- |\n",self.id,self.report["scope"].as_str().unwrap(),self.report["source"]["commit"].as_str().unwrap(),self.report["source"]["dirty"]);
        for s in self.report["stages"].as_array().unwrap() {
            md.push_str(&format!(
                "| {} | {} | {} | [{}]({}) |\n",
                s["name"].as_str().unwrap(),
                s["status"].as_str().unwrap(),
                s["durationMs"],
                s["log"].as_str().unwrap_or("reason"),
                s["log"].as_str().unwrap_or("report.json")
            ));
        }
        for exception in self.report["acceptanceExceptions"].as_array().unwrap() {
            md.push_str(&format!(
                "\nAcceptance exception: `{}` is `{}`; reason: `{}` (Issue #{}).\n",
                exception["name"].as_str().unwrap(),
                exception["status"].as_str().unwrap(),
                exception["reason"].as_str().unwrap(),
                exception["issue"]
            ));
        }
        md.push_str(&format!("\nSuccess for this scope: `{}`\n\nEnvironment, commands, reasons, test-result references and artifact identities: [report.json](report.json).\n\nIssue #3 development success does not establish whole-product, real-EXE, performance, DPI or appearance acceptance.\n",self.report["success"]));
        fs::write(self.directory.join("report.md"), md)
    }
    fn stage(&mut self, stage: &str) -> bool {
        match stage {
            "check" => {
                self.cargo("format", &["fmt", "--all", "--check"])
                    && self.cargo(
                        "rust-check",
                        &[
                            "clippy",
                            "--locked",
                            "--workspace",
                            "--offline",
                            "--all-targets",
                            "--",
                            "-D",
                            "warnings",
                        ],
                    )
                    && self.npm("typescript", &["run", "check"])
                    && self.node("static-contracts", &["test-support/static.mjs"])
            }
            "test" => {
                self.cargo(
                    "core-tests",
                    &["test", "--locked", "--offline", "-p", "legacists-core"],
                ) && self.cargo(
                    "application-tests",
                    &["test", "--locked", "--offline", "-p", "legacists-desktop"],
                ) && self.powershell("core-isolation", "test-support/core-isolation.ps1")
            }
            "ui" => self.node(
                "ui",
                &[
                    "apps/desktop/node_modules/@playwright/test/cli.js",
                    "test",
                    "--config",
                    "test-support/playwright.ui.config.mjs",
                ],
            ),
            "build" => {
                if !self.node("licenses", &["test-support/licenses.mjs"])
                    || !self.npm("frontend-build", &["run", "build"])
                    || !self.cargo(
                        "release-build",
                        &[
                            "build",
                            "--release",
                            "--locked",
                            "--offline",
                            "-p",
                            "legacists-desktop",
                        ],
                    )
                {
                    return false;
                }
                let artifacts = self.directory.join("artifacts");
                fs::create_dir_all(&artifacts).unwrap();
                let exe = artifacts.join("legacists-desktop.exe");
                fs::copy(self.root.join("target/release/legacists-desktop.exe"), &exe).unwrap();
                if !self.powershell("package", "test-support/package.ps1") {
                    return false;
                }
                for path in [exe, artifacts.join("legacists-windows-x64.zip")] {
                    let bytes = fs::read(&path).unwrap();
                    self.report["artifacts"].as_array_mut().unwrap().push(json!({"path":path,"bytes":bytes.len(),"sha256":format!("{:x}",Sha256::digest(&bytes))}));
                }
                true
            }
            "exe" | "exe-min" => {
                let args = if stage == "exe-min" {
                    vec![
                        "apps/desktop/node_modules/@playwright/test/cli.js",
                        "test",
                        "--config",
                        "test-support/playwright.exe.config.mjs",
                        "--grep",
                        "@minimum",
                    ]
                } else {
                    vec![
                        "apps/desktop/node_modules/@playwright/test/cli.js",
                        "test",
                        "--config",
                        "test-support/playwright.exe.config.mjs",
                    ]
                };
                self.node(stage, &args)
            }
            "perf" => self.node("performance", &["test-support/performance.mjs"]),
            "setup" => {
                self.node("node-version", &["test-support/environment.mjs"])
                    && self.npm("dependency-install", &["ci"])
                    && self.cargo("dependency-fetch", &["fetch", "--locked"])
                    && self.node(
                        "playwright-install",
                        &[
                            "apps/desktop/node_modules/@playwright/test/cli.js",
                            "install",
                            "chromium",
                        ],
                    )
                    && self.node("licenses", &["test-support/licenses.mjs"])
                    && self.node("dependency-audit", &["test-support/audit.mjs"])
            }
            _ => false,
        }
    }
}

fn main() {
    let mode = env::args().nth(1).unwrap_or_else(|| "help".into());
    if mode == "help" {
        println!("cargo xtask setup | check | test | ui | build | exe | perf | dev | ci\nDevelopment acceptance: dev; whole verification: ci. Real-EXE integration is optional for the initial release; run exe explicitly. No dependencies are installed by test commands.");
        return;
    }
    let mut run = Run::new(&mode).unwrap();
    let stages: Vec<&str> = match mode.as_str() {
        "dev" => vec!["check", "test", "ui", "build"],
        "ci" => vec!["check", "test", "ui", "build", "perf"],
        "exe" | "exe-min" | "perf" => {
            if let Some(path) = env::args().nth(2) {
                env::set_var("LEGACISTS_EXISTING_EXE", path);
            }
            vec![mode.as_str()]
        }
        _ => vec![mode.as_str()],
    };
    let mut success = true;
    for stage in stages {
        if success {
            success = run.stage(stage);
        } else {
            run.report["stages"].as_array_mut().unwrap().push(json!({"name":stage,"status":"not_run","reason":"preceding-required-stage-failed","durationMs":0}));
        }
    }
    run.report["success"] = json!(success);
    let end_fingerprint = fingerprint(&run.root).unwrap_or_default();
    if run.report["source"]["fingerprint"].as_str() != Some(&end_fingerprint) {
        run.report["source"]["changedDuringRun"] = json!(true);
        run.report["success"] = json!(false);
        success = false;
    }
    run.report["source"]["endFingerprint"] = json!(end_fingerprint);
    run.report["testResults"] = json!([
        "ui-results.json",
        "exe-results.json",
        "performance-results.json"
    ]
    .into_iter()
    .filter(|file| run.directory.join(file).exists())
    .collect::<Vec<_>>());
    run.report["endedAtUnixMs"] = json!(timestamp());
    run.save().unwrap();
    println!(
        "Result: {}\nReport: {}",
        if success { "passed" } else { "failed" },
        run.directory.join("report.md").display()
    );
    if !success {
        std::process::exit(1);
    }
}
