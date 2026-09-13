//! 计划任务（Autoruns "Scheduled Tasks" 标签页）：schtasks 全量 + 执行命令解析 + 禁用态。
//! 表头兼容中英文系统（任务名/TaskName、要运行的任务/Task To Run 等）。

use super::util::{Entry, Scanner, child_cmd, decode_console, extract_exe, split_csv_line};

/// 在表头行里找列号：中英文系统各给一组候选关键词。
fn find_col(cols: &[String], candidates: &[&str]) -> Option<usize> {
    cols.iter()
        .position(|c| candidates.iter().any(|kw| c.contains(kw)))
}

pub fn scan(sc: &mut Scanner) {
    let output = child_cmd("schtasks")
        .args(["/query", "/fo", "csv", "/v"])
        .output();
    let Ok(o) = output else { return };
    if !o.status.success() {
        return;
    }
    let text = decode_console(&o.stdout);
    let mut lines = text.lines();
    let Some(header) = lines.next() else { return };
    let cols = split_csv_line(header);
    // 中文系统：任务名 / 要运行的任务 / 作为用户运行 / 计划任务状态
    let Some(ti_task) = find_col(&cols, &["任务名", "TaskName"]) else {
        return;
    };
    let Some(ti_run) = find_col(&cols, &["要运行的任务", "Task To Run"]) else {
        return;
    };
    let ti_user = find_col(&cols, &["作为用户运行", "Run As User"]).unwrap_or(usize::MAX);
    let ti_state = find_col(&cols, &["计划任务状态", "Scheduled Task State"]);
    for line in lines {
        let row = split_csv_line(line);
        if row.len() <= ti_run {
            continue;
        }
        let task = row[ti_task].trim().to_string();
        let run_cmd = row[ti_run].trim().to_string();
        let user = row
            .get(ti_user)
            .map(String::as_str)
            .unwrap_or("?")
            .to_string();
        if run_cmd.is_empty() || run_cmd.eq_ignore_ascii_case("n/a") {
            continue;
        }
        let state = ti_state
            .and_then(|i| row.get(i).map(|s| s.trim().to_string()))
            .unwrap_or_default();
        let disabled = state.contains("已禁用") || state.eq_ignore_ascii_case("disabled");
        let exe = extract_exe(&run_cmd);
        let mut e = Entry::new(
            "计划任务",
            &task,
            format!("Run: {run_cmd} | As: {user}"),
            "info",
            exe,
        )
        .op_key(format!("task\u{1f}{task}"));
        if disabled {
            e = e.disabled();
        }
        sc.push_entry(e);
    }
}
