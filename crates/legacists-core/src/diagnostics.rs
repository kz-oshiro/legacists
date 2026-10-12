use crate::Diagnostic;

pub struct Guidance {
    pub impact: &'static str,
    pub check: &'static str,
    pub action: &'static str,
}

pub fn guidance(code: &str) -> Guidance {
    let (impact, check, action) = match code {
        "LEG-1001" => ("対象を読み取れず、関係は不完全です。", "相対パスの存在・読取権限・他の処理による更新を確認してください。", "読める状態にして再解析してください。"),
        "LEG-1002" => ("リンクまたは範囲外を読み取りませんでした。", "基準フォルダとリンクの実体を端末内で確認してください。", "実体を含む適切なフォルダを開くか、除外を維持してください。"),
        "LEG-2001" => ("本文と関係を取得できません。", "指定方式・BOM・端末のエディターでの表示を確認してください。", "元ファイルを書き換えずUTF-8またはSJIS系を手動指定して再解析してください。"),
        "LEG-2002" => ("文字コード判定が曖昧で、UTF-8を仮選択しました。", "文字コード欄と端末内の日本語を確認してください。", "必要なら文字コードを手動指定し、再解析してください。"),
        "LEG-3001" => ("この範囲は未解析で、関係が不完全です。", "行・列と対応記法、文字コードを端末内で確認してください。原因候補は構文不正・未対応構文・領域分離失敗です。", "文字コードを確認して再解析し、言語と構文の種類を記録してください。"),
        "LEG-4001" => ("候補または未解決の関係があります。解析器の異常とは限りません。", "参照先の存在・重複・除外・Webルート・対応規則を確認してください。", "設定を直して再解析し、外部依存や動的参照の場合はその限界を記録してください。"),
        "LEG-5001" => ("古い結果でのTXT保存を停止しました。", "変更された相対パスと変更種別を確認してください。", "再解析して選択とプレビューを確認してください。"),
        "LEG-5002" => ("TXTの全部または一部が未保存です。", "保存先の権限・空き容量・同名ファイル・ロック、成功済みと未完了の連番を確認してください。", "保存先を選び直してください。"),
        "LEG-6001" => ("設定の読込・保存・移行・削除が未完了です。", "設定画面の現在・予定保存先、権限・ロック・二重起動を確認してください。", "終了・再起動して再試行してください。無関係なフォルダを削除しないでください。"),
        _ => ("ジョブが失敗しました。原因は不明です。", "アプリ版・環境・処理段階・再現操作を確認してください。", "再起動して同じ操作を一度試してください。"),
    };
    Guidance {
        impact,
        check,
        action,
    }
}

/// Only allowlisted fields; no source, reference strings, options, or raw errors.
pub fn render_diagnostics(
    diagnostics: &[Diagnostic],
    app_version: &str,
    environment: &str,
) -> String {
    let mut out = format!("LEGACISTS-DIAGNOSTICS/1\nアプリ版: {app_version}\n環境: {environment}\n解析条件: 静的解析・UTF-8／SJIS系\n\n");
    for d in diagnostics {
        let g = guidance(&d.code);
        out.push_str(&format!("番号: {}\n段階: {}\n原因区分: {}\n対象: {}\n位置: {}\n影響: {}\n確認: {}\n対処: {}\n\n", d.code, d.stage, d.reason, d.path.as_ref().map(|p| serde_json::to_string(p).unwrap()).unwrap_or_else(|| "プロジェクト".into()), d.start.as_ref().map(|p| format!("{}行{}列", p.line, p.column)).unwrap_or_else(|| "不明".into()), g.impact, g.check, g.action));
    }
    out.push_str("解決しない場合: 番号・アプリ版・環境・再現操作・確認結果・試した方式と言語／構文の種類を報告してください。公開可能な合成再現例を使用し、実ソースの提出は不要です。相対パス等にも機密性があるため、公開Issueへ転記する前に内容を確認してください。\n確認結果: \n再現操作: \n未判明の事項: \n");
    out
}
