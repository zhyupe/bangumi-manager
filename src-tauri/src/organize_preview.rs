use crate::{copy, db::Store, model::*, paths, resources};
use std::path::Path;

fn build(store: &Store, mappings: Vec<Mapping>, progress: impl Fn(usize, usize)) -> Result<Plan> {
    let mut plan = copy::build_plan(store, mappings, progress)?;
    for i in 0..plan.rows.len() {
        plan.rows[i].backups = copy::backup_candidates(&plan, i)?;
    }
    Ok(plan)
}

pub fn preview(
    store: &Store,
    mappings: Vec<Mapping>,
    progress: impl Fn(usize, usize),
) -> Result<Plan> {
    let plan = build(store, mappings, progress)?;
    store.put("organize_previews", &plan.id, &plan)?;
    Ok(plan)
}

// Row IDs change when rebuilding. Compare every file operation and review warning instead.
fn contents(plan: &Plan) -> serde_json::Value {
    serde_json::json!({
        "settings": plan.settings,
        "works": plan.works,
        "rows": plan.rows.iter().map(|row| serde_json::json!({
            "source": row.source, "target": row.target, "work": row.work_id,
            "episodes": row.episode_ids, "part": row.part, "extra": row.extra, "attachment": row.attachment,
            "signature": row.signature, "target_signature": row.target_signature,
            "conflicts": row.conflicts, "warnings": row.warnings, "subtitle": row.subtitle,
            "folder": row.media_folder, "operation": row.operation,
            "backups": row.backups.iter().map(|b| (&b.original, &b.signature)).collect::<Vec<_>>()
        })).collect::<Vec<_>>()
    })
}

pub fn submit(store: &Store, id: &str, choices: Vec<Decision>) -> Result<Plan> {
    store.diagnostics.measure(
        "organize.submit",
        serde_json::json!({"task_id": id}),
        || {
            // A lost response can be retried without creating another task.
            if let Some(plan) = store.get("plans", id)? {
                return Ok(plan);
            }
            let mut plan: Plan = store
                .get("organize_previews", id)?
                .ok_or("预览不存在，请重新生成")?;
            let mappings = plan
                .rows
                .iter()
                .filter(|r| r.parent_id.is_none())
                .map(|r| Mapping {
                    part: r.part,
                    source: r.source.clone(),
                    work_id: r.work_id.clone(),
                    episode_ids: r.episode_ids.clone(),
                    extra: r.extra,
                })
                .collect();
            let fresh = build(store, mappings, |_, _| {})?;
            if contents(&fresh) != contents(&plan) {
                return Err("文件、关联或整理规则在预览后变化，请重新生成预览并核对".into());
            }
            copy::apply_decisions(&mut plan, choices)?;
            copy::preflight(store, &plan)?;
            for row in plan.rows.iter().filter(|r| r.action != "skip") {
                if resources::regular(Path::new(&row.source), None)? != row.signature
                    || paths::optional_signature(Path::new(&row.target))? != row.target_signature
                {
                    return Err("文件、关联或整理规则在预览后变化，请重新生成预览并核对".into());
                }
            }
            store.put("plans", id, &plan)?;
            store.remove("organize_previews", id)?;
            Ok(plan)
        },
    )
}
