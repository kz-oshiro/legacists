use legacists_core::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

#[derive(Default)]
struct Memory {
    files: BTreeMap<String, Vec<u8>>,
    fail: Option<String>,
    reads: AtomicUsize,
    cancel_on: Option<usize>,
}
impl Memory {
    fn text(files: &[(&str, &str)]) -> Self {
        Self {
            files: files
                .iter()
                .map(|(p, s)| (p.to_string(), s.as_bytes().to_vec()))
                .collect(),
            ..Default::default()
        }
    }
}
impl InputProvider for Memory {
    fn enumerate(
        &self,
        options: &AnalysisOptions,
        cancel: &Cancellation,
    ) -> Result<input::Listing, InputError> {
        if cancel.is_cancelled() {
            return Err(InputError::Cancelled);
        }
        Ok(input::Listing {
            paths: self
                .files
                .keys()
                .filter(|p| !input::excluded(p, options) && language(p).is_some())
                .cloned()
                .collect(),
            skipped: Vec::new(),
        })
    }
    fn read(&self, path: &str, cancel: &Cancellation) -> Result<Vec<u8>, InputError> {
        let n = self.reads.fetch_add(1, Ordering::SeqCst) + 1;
        if self.cancel_on == Some(n) {
            cancel.cancel();
            return Err(InputError::Cancelled);
        }
        if self.fail.as_deref() == Some(path) {
            return Err(InputError::Access);
        }
        self.files.get(path).cloned().ok_or(InputError::Missing)
    }
}
fn run(input: &Memory) -> AnalysisSnapshot {
    analyze(
        AnalysisOptions::default(),
        input,
        &Cancellation::default(),
        |_| {},
    )
    .unwrap()
}
fn triples(s: &AnalysisSnapshot, status: Resolution) -> BTreeSet<(String, String, String)> {
    s.relations
        .iter()
        .filter(|r| r.status == status)
        .flat_map(|r| {
            r.targets.iter().map(move |t| {
                (
                    s.resource(&r.from).unwrap().path.clone(),
                    r.kind.clone(),
                    s.resource(t).unwrap().path.clone(),
                )
            })
        })
        .collect()
}
fn fixture() -> FileInput {
    FileInput::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/mixed")).unwrap()
}

#[test]
fn t02_mixed_exact_relations_and_original_evidence() {
    let s = analyze(
        AnalysisOptions::default(),
        &fixture(),
        &Cancellation::default(),
        |_| {},
    )
    .unwrap();
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/mixed/manifest.json")).unwrap();
    let expected: BTreeSet<_> = manifest["confirmed"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r[0].as_str().unwrap().into(),
                r[1].as_str().unwrap().into(),
                r[2].as_str().unwrap().into(),
            )
        })
        .collect();
    assert_eq!(triples(&s, Resolution::Confirmed), expected);
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    for row in manifest["evidence"].as_array().unwrap() {
        let path = row[0].as_str().unwrap();
        let rule = row[1].as_str().unwrap();
        let needle = row[2].as_str().unwrap();
        assert!(
            s.evidences
                .iter()
                .any(|e| s.resource(&e.resource_id).unwrap().path == path
                    && e.rule == rule
                    && &s.source(&e.resource_id).unwrap()[e.start.byte..e.end.byte] == needle),
            "{row}"
        );
    }
    let route = s.relations.iter().find(|r| r.kind == "navigation").unwrap();
    assert!(route.evidence_ids.len() >= 4);
}

#[test]
fn t03_java_ambiguity_scope_type_parameters_and_comments() {
    let m=Memory::text(&[("p/A.java","package p; import q.*; class A<T> { T value; B b; /* Ghost g; */ String s=\"new Ghost()\"; }"),("q/B.java","package q; class B {}"),("a/X.java","package a; class X {}"),("a/X2.java","package a; class X {}")]);
    let s = run(&m);
    assert!(!s
        .relations
        .iter()
        .any(|r| r.reference == "T" || r.reference == "Ghost"));
    let b = s.relations.iter().find(|r| r.reference == "B").unwrap();
    assert_eq!(b.status, Resolution::Candidate);
    assert_eq!(b.targets.len(), 1);
    assert!(s
        .relations
        .iter()
        .any(|r| r.reference == "String" && r.status == Resolution::Unresolved));
}

#[test]
fn t04_old_new_java_and_unicode_positions() {
    let m = Memory::text(&[
        ("p/Type.java", "package p; public interface Type {}"),
        (
            "p/A.java",
            r#"package p; @Deprecated sealed class A<T extends Type> permits B { Type field; Class<?> clazz = Type.class; void f(){ java.util.function.Supplier<Type> x=()->new B(); Object a=(Type)null; boolean yes=a instanceof Type; } } final class B extends A<Type> {} record R(Type a) {} enum E { ONE }"#,
        ),
        (
            "p/Escaped.java",
            r"package p; class Escaped { T\u0079pe value; }",
        ),
    ]);
    let s = run(&m);
    assert!(
        s.resources.iter().all(|r| r.parse == "ok"),
        "{:?}",
        s.diagnostics
    );
    let escaped = s
        .resources
        .iter()
        .find(|r| r.path == "p/Escaped.java")
        .unwrap();
    let relation = s
        .relations
        .iter()
        .find(|r| r.from == escaped.id && r.reference == "Type")
        .unwrap();
    assert_eq!(relation.status, Resolution::Confirmed);
    let e = s
        .evidences
        .iter()
        .find(|e| relation.evidence_ids.contains(&e.id))
        .unwrap();
    assert_eq!(
        &s.source(&escaped.id).unwrap()[e.start.byte..e.end.byte],
        r"T\u0079pe"
    );
}

#[test]
fn t04_jsp_mixed_comments_and_facelets_namespaces() {
    let m = Memory::text(&[
        (
            "web/A.jsp",
            r#"<%-- <script src="bad.js"></script><%@ include file="bad.jspf" %> --%><!-- <script src="bad.js"></script><%@ include file="good.jspf" %><jsp:include page="good.jspf"/> --><% p.Type value=null; %><ui:include src="bad.jspf"/>"#,
        ),
        ("web/good.jspf", "good"),
        ("p/Type.java", "package p; class Type {}"),
        (
            "web/A.xhtml",
            r#"<h:html xmlns:h="http://www.w3.org/1999/xhtml" xmlns:z="jakarta.faces.facelets"><z:include src="good.jspf"/><fake:include xmlns:fake="urn:unknown" src="bad.jspf"/></h:html>"#,
        ),
    ]);
    let s = run(&m);
    assert!(!s.relations.iter().any(|r| r.reference.contains("bad")));
    assert!(s
        .relations
        .iter()
        .any(|r| r.reference == "p.Type" && r.status == Resolution::Confirmed));
    assert!(s.relations.iter().any(
        |r| r.reference == "good.jspf" && s.resource(&r.from).unwrap().path.ends_with("xhtml")
    ));
}

#[test]
fn t04_js_module_and_shadowed_require() {
    let m = Memory::text(&[
        (
            "a.js",
            r#"import './b.js'; export {b} from './b.js'; import('./b.js'); require('./b'); require(expr); function f(require){require('./b.js');}"#,
        ),
        ("b.js", "export const b=1;"),
    ]);
    let s = run(&m);
    assert!(s
        .relations
        .iter()
        .any(|r| r.reference == "./b.js" && r.status == Resolution::Confirmed));
    assert!(s
        .relations
        .iter()
        .any(|r| r.reference == "./b" && r.status == Resolution::Candidate));
    assert!(s
        .relations
        .iter()
        .any(|r| r.reason == "local-require" && r.status == Resolution::Candidate));
    assert!(s
        .relations
        .iter()
        .any(|r| r.status == Resolution::Unresolved));
}

#[test]
fn t05_config_unicode_continuation_unknown_and_external_entities() {
    let m = Memory::text(&[
        ("p/A.java", "package p; class A {}"),
        (
            "a.properties",
            "handler.class=p.\\\n  \\u0041\nunknown=p.A\npassword=SOURCE_SECRET\n",
        ),
        (
            "a.xml",
            r#"<!DOCTYPE root SYSTEM "http://127.0.0.1:1/never"><root><class>p.A</class><unknown>p.A</unknown><class>&external;</class></root>"#,
        ),
    ]);
    let s = run(&m);
    assert!(s
        .relations
        .iter()
        .any(|r| r.reference == "p.A" && r.status == Resolution::Confirmed));
    assert!(s
        .relations
        .iter()
        .any(|r| r.reference == "p.A" && r.status == Resolution::Candidate));
    assert!(!s.relations.iter().any(|r| r.reference == "SOURCE_SECRET"));
    assert!(s
        .relations
        .iter()
        .any(|r| r.status == Resolution::Unresolved && r.reason == "dynamic-or-entity"));
}

#[test]
fn t07_strict_encoding_and_manual_no_fallback() {
    let text = "// ①髙﨑 ｶﾅ\r\nclass A {}";
    let (sjis, _, errors) = encoding_rs::SHIFT_JIS.encode(text);
    assert!(!errors);
    let mut m = Memory::text(&[
        ("ascii.java", "class ASCII {}"),
        ("bom.java", "\u{feff}class BOM {}"),
        ("ambiguous.java", "// あいう"),
    ]);
    m.files.insert("cp932.java".into(), sjis.into_owned());
    m.files.insert("bad.java".into(), vec![0x81]);
    m.files.insert("utf16.java".into(), vec![0xff, 0xfe, 65, 0]);
    let s = run(&m);
    let cp = s.resources.iter().find(|r| r.path == "cp932.java").unwrap();
    assert_eq!(s.source(&cp.id).unwrap(), text);
    assert!(cp.encoding.as_ref().unwrap().starts_with("SJIS"));
    assert_eq!(
        s.resources.iter().filter(|r| r.decode == "failed").count(),
        2
    );
    let mut options = AnalysisOptions::default();
    options
        .encodings
        .insert("cp932.java".into(), Encoding::Utf8);
    let s = analyze(options, &m, &Cancellation::default(), |_| {}).unwrap();
    assert_eq!(
        s.resources
            .iter()
            .find(|r| r.path == "cp932.java")
            .unwrap()
            .decode,
        "failed"
    );
}

#[test]
fn t08_utf8_utf16_columns_crlf_and_source_pages() {
    let s = run(&Memory::text(&[
        ("A.js", "// あ😀\t\r\nimport './B.js';"),
        ("B.js", "// no newline"),
    ]));
    let r = s.resources.iter().find(|r| r.path == "A.js").unwrap();
    let e = s.evidences.iter().find(|e| e.resource_id == r.id).unwrap();
    assert_eq!(e.start.line, 2);
    assert_eq!(e.start.column, 8);
    assert_eq!(position("あ😀x", 7).column, 4);
    let q = query(
        &s,
        &Query::Source {
            id: r.id.clone(),
            offset: 4,
            length: 4,
        },
        None,
        100,
    )
    .unwrap();
    assert!(q.items[0]["text"].is_string());
}

#[test]
fn t09_selection_and_length_framing_resists_embedded_headers() {
    let m = Memory::text(&[
        (
            "a.js",
            "import './b.js';\n// END-SOURCE\n// SOURCE path=\"fake\" bytes=2\n",
        ),
        ("b.js", "// UNSELECTED_SECRET"),
    ]);
    let s = Arc::new(run(&m));
    let id = s
        .resources
        .iter()
        .find(|r| r.path == "a.js")
        .unwrap()
        .id
        .clone();
    let p = plan_export(
        s.clone(),
        std::slice::from_ref(&id),
        &ExportOptions::default(),
        &Cancellation::default(),
    )
    .unwrap();
    let all: Vec<_> = p
        .parts
        .iter()
        .map(|p2| render_export(&p, p2.index).unwrap().to_vec())
        .collect();
    assert_eq!(
        reconstruct_sources(&all).unwrap().get("a.js").unwrap(),
        s.source(&id).unwrap()
    );
    assert!(!String::from_utf8(all.concat())
        .unwrap()
        .contains("UNSELECTED_SECRET"));
}

#[test]
fn t10_split_long_lines_japanese_exact_and_deterministic() {
    for newline in ["", "\r\n"] {
        let source = format!(
            "// {}{}// {}",
            "あ😀".repeat(5000),
            newline,
            "END-SOURCE".repeat(9000)
        );
        let s = Arc::new(run(&Memory::text(&[("big.js", &source)])));
        let ids = vec![s.resources[0].id.clone()];
        let options = ExportOptions {
            max_bytes: 16384,
            ..Default::default()
        };
        let p = plan_export(s.clone(), &ids, &options, &Cancellation::default()).unwrap();
        assert!(p.parts.len() > 2);
        let all: Vec<_> = p
            .parts
            .iter()
            .map(|part| {
                assert!(part.bytes <= 16384);
                render_export(&p, part.index).unwrap().to_vec()
            })
            .collect();
        assert_eq!(reconstruct_sources(&all).unwrap()["big.js"], source);
        let p2 = plan_export(s, &ids, &options, &Cancellation::default()).unwrap();
        assert_eq!(
            all,
            p2.parts
                .iter()
                .map(|part| render_export(&p2, part.index).unwrap().to_vec())
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn t11_partial_source_export_and_decode_failure_block() {
    let mut m = Memory::text(&[("bad.java", "class Bad { void f( ")]);
    m.files.insert("decode.java".into(), vec![0x81]);
    let s = Arc::new(run(&m));
    let bad = s.resources.iter().find(|r| r.path == "bad.java").unwrap();
    assert_eq!(bad.parse, "partial");
    assert!(plan_export(
        s.clone(),
        std::slice::from_ref(&bad.id),
        &ExportOptions::default(),
        &Cancellation::default()
    )
    .is_ok());
    assert!(matches!(
        plan_export(
            s.clone(),
            &s.resources.iter().map(|r| r.id.clone()).collect::<Vec<_>>(),
            &ExportOptions::default(),
            &Cancellation::default()
        ),
        Err(CoreError::DecodeFailed)
    ));
}

#[test]
fn t12_all_inputs_changes_even_unselected_same_length() {
    let mut m = Memory::text(&[("a.js", "import './b.js';"), ("b.js", "// one")]);
    let s = run(&m);
    assert!(verify_inputs(&s, &m, &Cancellation::default())
        .unwrap()
        .is_empty());
    m.files.insert("b.js".into(), b"// two".to_vec());
    assert_eq!(
        verify_inputs(&s, &m, &Cancellation::default()).unwrap()[0].kind,
        "changed"
    );
    m.files.insert("c.js".into(), b"// new".to_vec());
    m.files.remove("a.js");
    assert_eq!(
        verify_inputs(&s, &m, &Cancellation::default())
            .unwrap()
            .len(),
        3
    );
}

#[test]
fn t13_cancel_at_read_and_wrong_snapshot_cursor() {
    let m = Memory {
        cancel_on: Some(1),
        ..Memory::text(&[("a.js", "// a")])
    };
    assert!(matches!(
        analyze(
            AnalysisOptions::default(),
            &m,
            &Cancellation::default(),
            |_| {}
        ),
        Err(CoreError::Cancelled)
    ));
    let s = run(&Memory::text(&[("a.js", "// a"), ("b.js", "// b")]));
    let q = Query::Resources {
        search: String::new(),
        parent: None,
    };
    let page = query(&s, &q, None, 1).unwrap();
    assert!(page.omitted);
    assert_eq!(
        query(&s, &q, page.next_cursor.as_deref(), 1)
            .unwrap()
            .items
            .len(),
        1
    );
    assert!(matches!(
        query(
            &s,
            &Query::Resources {
                search: "a".into(),
                parent: None
            },
            page.next_cursor.as_deref(),
            1
        ),
        Err(CoreError::InvalidCursor)
    ));
}

#[test]
fn t14_report_allowlist_and_actionable_diagnostics() {
    let s = run(&Memory::text(&[
        ("bad.xml", "<class>SECRET_CONFIG&secret;</class>"),
        ("bad.java", "// SECRET_SOURCE\nclass { "),
    ]));
    let report =
        diagnostics::render_diagnostics(&s.diagnostics, "0.1.0", "Windows 11 x64; WebView2 test");
    assert!(report.contains("LEG-3001"));
    assert!(!report.contains("SECRET_CONFIG"));
    assert!(!report.contains("SECRET_SOURCE"));
    for word in [
        "影響:",
        "確認:",
        "対処:",
        "再現操作:",
        "実ソースの提出は不要",
    ] {
        assert!(report.contains(word));
    }
}

#[test]
fn t01_unreadable_and_traversal_options() {
    let m = Memory {
        fail: Some("a.java".into()),
        ..Memory::text(&[("a.java", "class A {}")])
    };
    let s = run(&m);
    assert!(s.diagnostics.iter().any(|d| d.code == "LEG-1001"));
    let options = AnalysisOptions {
        web_root: Some("../outside".into()),
        ..Default::default()
    };
    assert!(matches!(
        analyze(options, &m, &Cancellation::default(), |_| {}),
        Err(CoreError::InputBoundary)
    ));
    assert!(!input::relative_path("../outside.java"));
    assert!(!input::relative_path("C:/outside.java"));
}

#[test]
fn t06_aggregation_pagination_preserves_relations() {
    let mut m = Memory::text(&[("p/B.java", "package p; class B {}")]);
    for i in 0..30 {
        m.files.insert(
            format!("p/A{i}.java"),
            format!("package p; class A{i} {{ B b; }}").into_bytes(),
        );
    }
    let s = run(&m);
    let id = s
        .resources
        .iter()
        .find(|r| r.path == "p/B.java")
        .unwrap()
        .id
        .clone();
    let q = Query::Neighbors {
        root: id,
        direction: "in".into(),
        kinds: Vec::new(),
        statuses: Vec::new(),
        depth: 1,
    };
    let mut cursor = None;
    let mut count = 0;
    loop {
        let result = query(&s, &q, cursor.as_deref(), 7).unwrap();
        assert_eq!(result.total, 30);
        count += result.items.len();
        cursor = result.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(count, 30);
    assert_eq!(s.relations.len(), 30);
    let group = query(
        &s,
        &Query::Overview {
            grouping: "folder".into(),
            group: None,
        },
        None,
        100,
    )
    .unwrap();
    assert_eq!(group.items[0]["internal"].as_array().unwrap().len(), 30);
}

#[test]
fn t03_missing_nested_import_is_not_confirmed_as_its_outer_type() {
    let s = run(&Memory::text(&[
        ("p/Outer.java", "package p; public class Outer {}"),
        (
            "q/Use.java",
            "package q; import p.Outer.Missing; class Use {}",
        ),
    ]));
    let r = s.relations.iter().find(|r| r.kind == "import").unwrap();
    assert_eq!(r.status, Resolution::Unresolved);
    assert!(r.targets.is_empty());
}

#[test]
fn t04_js_escaped_literal_modules_preserve_original_evidence() {
    let original = "import './\\u0061.js'; import './\\x62.js';";
    let s = run(&Memory::text(&[
        ("main.js", original),
        ("a.js", "// a"),
        ("b.js", "// b"),
    ]));
    assert_eq!(
        triples(&s, Resolution::Confirmed),
        BTreeSet::from([
            ("main.js".into(), "module".into(), "a.js".into()),
            ("main.js".into(), "module".into(), "b.js".into())
        ])
    );
    assert!(s
        .evidences
        .iter()
        .any(|e| &original[e.start.byte..e.end.byte] == "'./\\u0061.js'"));
}

#[test]
fn t05_servlet_priority_exact_prefix_extension_default_and_welcome_root() {
    let xml = r#"<web-app><servlet><servlet-name>exact</servlet-name><servlet-class>app.Exact</servlet-class></servlet><servlet><servlet-name>prefix</servlet-name><servlet-class>app.Prefix</servlet-class></servlet><servlet><servlet-name>extension</servlet-name><servlet-class>app.Extension</servlet-class></servlet><servlet><servlet-name>default</servlet-name><servlet-class>app.Default</servlet-class></servlet><servlet-mapping><servlet-name>default</servlet-name><url-pattern>/</url-pattern></servlet-mapping><servlet-mapping><servlet-name>extension</servlet-name><url-pattern>*.do</url-pattern></servlet-mapping><servlet-mapping><servlet-name>prefix</servlet-name><url-pattern>/api/*</url-pattern></servlet-mapping><servlet-mapping><servlet-name>exact</servlet-name><url-pattern>/api/exact.do</url-pattern></servlet-mapping><welcome-file-list><welcome-file>home.jsp</welcome-file></welcome-file-list></web-app>"#;
    let m = Memory::text(&[
        ("web/WEB-INF/web.xml", xml),
        ("web/home.jsp", ""),
        (
            "web/start.jsp",
            r#"<a href="/api/exact.do"></a><a href="/api/other.do"></a><a href="/other.do"></a><a href="/else"></a>"#,
        ),
        ("src/Exact.java", "package app; class Exact {}"),
        ("src/Prefix.java", "package app; class Prefix {}"),
        ("src/Extension.java", "package app; class Extension {}"),
        ("src/Default.java", "package app; class Default {}"),
    ]);
    let s = run(&m);
    for (reference, target) in [
        ("/api/exact.do", "src/Exact.java"),
        ("/api/other.do", "src/Prefix.java"),
        ("/other.do", "src/Extension.java"),
        ("/else", "src/Default.java"),
    ] {
        let r = s
            .relations
            .iter()
            .find(|r| r.reference == reference && r.kind == "navigation")
            .unwrap();
        assert_eq!(r.status, Resolution::Confirmed, "{reference}");
        assert_eq!(s.resource(&r.targets[0]).unwrap().path, target);
    }
    // The default servlet route also takes precedence over a physical welcome JSP.
    let welcome = s
        .relations
        .iter()
        .find(|r| {
            r.evidence_ids.iter().any(|id| {
                s.evidences.iter().any(|e| {
                    &e.id == id
                        && e.rule == "WEBXML-ROUTE"
                        && e.resource_id == r.from
                        && xml[e.start.byte..e.end.byte].contains("home.jsp")
                })
            })
        })
        .unwrap();
    assert_eq!(
        s.resource(&welcome.targets[0]).unwrap().path,
        "src/Default.java"
    );
}

#[test]
fn t10_exact_capacity_and_oversized_metadata_are_framed() {
    let source = format!("// {}", "日😀".repeat(4000));
    let name = "literal bytes=5.js";
    let mut m = Memory::text(&[(name, &source)]);
    for i in 0..1500 {
        m.files.insert(
            format!("extra/resource-{i:06}-{}.js", "x".repeat(20)),
            b"// unselected".to_vec(),
        );
    }
    let s = Arc::new(run(&m));
    let ids = vec![s
        .resources
        .iter()
        .find(|r| r.path == name)
        .unwrap()
        .id
        .clone()];
    let plan = plan_export(
        s.clone(),
        &ids,
        &ExportOptions::default(),
        &Cancellation::default(),
    )
    .unwrap();
    let exact = plan.parts.iter().map(|p| p.bytes).max().unwrap();
    for capacity in [exact - 1, exact, exact + 1, 16384] {
        let plan = plan_export(
            s.clone(),
            &ids,
            &ExportOptions {
                max_bytes: capacity,
                ..Default::default()
            },
            &Cancellation::default(),
        )
        .unwrap();
        let parts: Vec<_> = plan
            .parts
            .iter()
            .map(|p| {
                assert!(p.bytes <= capacity);
                render_export(&plan, p.index).unwrap().to_vec()
            })
            .collect();
        assert_eq!(reconstruct_sources(&parts).unwrap()[name], source);
    }
}

#[test]
fn t13_cancel_each_analysis_stage_does_not_publish_result() {
    for stage in ["enumerate", "decode", "extract", "resolve"] {
        let cancel = Cancellation::default();
        let result = analyze(
            AnalysisOptions::default(),
            &Memory::text(&[("a.js", "import './b.js';"), ("b.js", "// b")]),
            &cancel,
            |progress| {
                if progress.stage == stage {
                    cancel.cancel();
                }
            },
        );
        assert!(matches!(result, Err(CoreError::Cancelled)), "{stage}");
    }
}

#[cfg(windows)]
#[test]
fn t01_windows_junction_and_outside_read_rejected_originals_unchanged() {
    let root = std::env::temp_dir().join(format!(
        "legacists-input-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let project = root.join("project");
    let outside = root.join("outside");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(project.join("inside.js"), b"// inside").unwrap();
    std::fs::write(outside.join("secret.js"), b"// outside secret").unwrap();
    let status=std::process::Command::new("pwsh").args(["-NoProfile","-Command","New-Item -ItemType Junction -Path $env:LEGACISTS_JUNCTION_LINK -Target $env:LEGACISTS_JUNCTION_TARGET | Out-Null; if(-not $?) {exit 1}"]).env("LEGACISTS_JUNCTION_LINK", project.join("link")).env("LEGACISTS_JUNCTION_TARGET", &outside).status().unwrap();
    assert!(status.success());
    let input = FileInput::new(&project).unwrap();
    let listing = input
        .enumerate(&AnalysisOptions::default(), &Cancellation::default())
        .unwrap();
    assert_eq!(listing.paths, vec!["inside.js"]);
    assert!(listing.skipped.iter().any(|s| s.path == "link"));
    assert!(matches!(
        input.read("link/secret.js", &Cancellation::default()),
        Err(InputError::Boundary)
    ));
    assert!(input
        .read("../outside/secret.js", &Cancellation::default())
        .is_err());
    assert_eq!(
        std::fs::read(outside.join("secret.js")).unwrap(),
        b"// outside secret"
    );
    std::fs::remove_dir(project.join("link")).unwrap();
    std::fs::remove_file(project.join("inside.js")).unwrap();
    std::fs::remove_file(outside.join("secret.js")).unwrap();
    std::fs::remove_dir(project).unwrap();
    std::fs::remove_dir(outside).unwrap();
    std::fs::remove_dir(root).unwrap();
}
