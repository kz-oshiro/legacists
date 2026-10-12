use crate::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Query {
    Resources {
        search: String,
        parent: Option<String>,
    },
    Resource {
        id: String,
    },
    Neighbors {
        root: String,
        direction: String,
        kinds: Vec<String>,
        statuses: Vec<Resolution>,
        depth: usize,
    },
    Overview {
        grouping: String,
        group: Option<String>,
    },
    Relation {
        id: String,
    },
    Source {
        id: String,
        offset: usize,
        length: usize,
    },
    Diagnostics,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryResult {
    pub items: Vec<Value>,
    pub resources: Vec<Resource>,
    pub total: usize,
    pub next_cursor: Option<String>,
    pub omitted: bool,
}

pub fn query(
    snapshot: &AnalysisSnapshot,
    query: &Query,
    cursor: Option<&str>,
    limit: usize,
) -> Result<QueryResult, CoreError> {
    if !(1..=500).contains(&limit) {
        return Err(CoreError::InvalidRange);
    }
    let fingerprint = input::hash(
        &serde_json::to_vec(&(snapshot.id.as_str(), query, limit))
            .map_err(|_| CoreError::Internal)?,
    );
    let offset = match cursor {
        Some(c) => {
            let (key, offset) = c.split_once(':').ok_or(CoreError::InvalidCursor)?;
            if key != fingerprint {
                return Err(CoreError::InvalidCursor);
            }
            offset
                .parse::<usize>()
                .map_err(|_| CoreError::InvalidCursor)?
        }
        None => 0,
    };
    let mut items = match query {
        Query::Resource { id } => vec![json!(snapshot.resource(id)?)],
        Query::Resources { search, parent } => {
            let needle = search.to_lowercase();
            snapshot
                .resources
                .iter()
                .filter(|r| {
                    r.path.to_lowercase().contains(&needle)
                        && parent
                            .as_ref()
                            .is_none_or(|p| r.path.starts_with(&format!("{p}/")))
                })
                .map(|r| json!(r))
                .collect::<Vec<_>>()
        }
        Query::Neighbors {
            root,
            direction,
            kinds,
            statuses,
            depth,
        } => {
            snapshot.resource(root)?;
            if *depth == 0 || *depth > 8 || !matches!(direction.as_str(), "in" | "out" | "both") {
                return Err(CoreError::InvalidRange);
            }
            let mut seen = BTreeSet::from([root.clone()]);
            let mut found = BTreeSet::new();
            let mut frontier = seen.clone();
            for _ in 0..*depth {
                let mut next = BTreeSet::new();
                for r in &snapshot.relations {
                    if !kinds.is_empty() && !kinds.contains(&r.kind)
                        || !statuses.is_empty() && !statuses.contains(&r.status)
                    {
                        continue;
                    }
                    let outgoing = direction != "in" && frontier.contains(&r.from);
                    let incoming =
                        direction != "out" && r.targets.iter().any(|id| frontier.contains(id));
                    if outgoing || incoming {
                        found.insert(r.id.clone());
                        next.insert(r.from.clone());
                        next.extend(r.targets.iter().cloned());
                    }
                }
                frontier = next.difference(&seen).cloned().collect();
                seen.extend(next);
                if frontier.is_empty() {
                    break;
                }
            }
            snapshot
                .relations
                .iter()
                .filter(|r| found.contains(&r.id))
                .map(|r| json!(r))
                .collect()
        }
        Query::Overview { grouping, group } => {
            if !matches!(grouping.as_str(), "folder" | "package") {
                return Err(CoreError::InvalidRange);
            }
            let mut groups = BTreeMap::<String, Vec<String>>::new();
            let mut membership = BTreeMap::new();
            for r in &snapshot.resources {
                let name = if grouping == "package" && r.language == "java" {
                    snapshot
                        .symbols
                        .iter()
                        .find(|s| s.resource_id == r.id)
                        .and_then(|s| s.name.rsplit_once('.'))
                        .map(|p| p.0.to_owned())
                        .unwrap_or_else(|| "(default)".into())
                } else {
                    r.path
                        .rsplit_once('/')
                        .map(|p| p.0.to_owned())
                        .unwrap_or_else(|| "(root)".into())
                };
                groups.entry(name.clone()).or_default().push(r.id.clone());
                membership.insert(r.id.clone(), name);
            }
            if let Some(group) = group {
                groups
                    .get(group)
                    .ok_or(CoreError::InvalidId)?
                    .iter()
                    .map(|id| json!(snapshot.resource(id).unwrap()))
                    .collect()
            } else {
                groups
                    .iter()
                    .map(|(name, members)| {
                        let mut internal = Vec::new();
                        let mut outgoing = BTreeMap::<String, Vec<String>>::new();
                        for r in &snapshot.relations {
                            if members.contains(&r.from) {
                                for target in &r.targets {
                                    if let Some(other) = membership.get(target) {
                                        if other == name {
                                            internal.push(r.id.clone());
                                        } else {
                                            outgoing
                                                .entry(other.clone())
                                                .or_default()
                                                .push(r.id.clone());
                                        }
                                    }
                                }
                            }
                        }
                        internal.sort();
                        internal.dedup();
                        for ids in outgoing.values_mut() {
                            ids.sort();
                            ids.dedup();
                        }
                        json!({"id":name,"members":members,"internal":internal,"outgoing":outgoing})
                    })
                    .collect()
            }
        }
        Query::Relation { id } => {
            let r = snapshot
                .relations
                .iter()
                .find(|r| &r.id == id)
                .ok_or(CoreError::InvalidId)?;
            let evidence:Vec<_>=r.evidence_ids.iter().filter_map(|id|snapshot.evidences.iter().find(|e|&e.id==id)).map(|e|{
                let source=snapshot.source(&e.resource_id).unwrap_or("");let excerpt=source.get(e.start.byte..e.end.byte).unwrap_or("");
                json!({"evidence":e,"path":snapshot.resource(&e.resource_id).unwrap().path,"excerpt":excerpt})
            }).collect();
            vec![json!({"relation":r,"evidence":evidence})]
        }
        Query::Source { id, offset, length } => {
            let source = snapshot.source(id)?;
            if *offset > source.len() || *length > 1_048_576 {
                return Err(CoreError::InvalidRange);
            }
            let mut start = *offset;
            while !source.is_char_boundary(start) {
                start -= 1;
            }
            let mut end = (*offset).saturating_add(*length).min(source.len());
            while !source.is_char_boundary(end) {
                end -= 1;
            }
            vec![
                json!({"resource":snapshot.resource(id)?,"text":&source[start..end],"start":start,"startPosition":position(source,start),"end":end,"totalBytes":source.len()}),
            ]
        }
        Query::Diagnostics => snapshot.diagnostics.iter().map(|d| json!(d)).collect(),
    };
    let total = items.len();
    if offset > total {
        return Err(CoreError::InvalidCursor);
    }
    let end = offset.saturating_add(limit).min(total);
    items = items.drain(offset..end).collect();
    let mut ids = BTreeSet::new();
    if matches!(query, Query::Neighbors { .. }) {
        for item in &items {
            if let Some(id) = item["from"].as_str() {
                ids.insert(id);
            }
            if let Some(targets) = item["targets"].as_array() {
                ids.extend(targets.iter().filter_map(Value::as_str));
            }
        }
    }
    let resources = ids
        .into_iter()
        .filter_map(|id| snapshot.resource(id).ok().cloned())
        .collect();
    let next_cursor = (end < total).then(|| format!("{fingerprint}:{end}"));
    Ok(QueryResult {
        items,
        resources,
        total,
        omitted: next_cursor.is_some(),
        next_cursor,
    })
}
