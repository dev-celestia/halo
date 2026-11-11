//! Pure DAG lane-layout algorithm for the commit graph.

pub(super) fn compute_graph_lanes(
    commits: Vec<halo_backend::commands::coder_git::GitCommitDto>,
) -> Vec<crate::work::GitGraphCommit> {
    let mut active_lanes: Vec<Option<String>> = Vec::new();
    let mut result = Vec::with_capacity(commits.len());

    for dto in commits {
        let sha = &dto.sha;
        let (lane_idx, has_incoming) =
            if let Some(idx) = active_lanes.iter().position(|l| l.as_deref() == Some(sha)) {
                (idx, true)
            } else if let Some(empty_idx) = active_lanes.iter().position(|l| l.is_none()) {
                active_lanes[empty_idx] = Some(sha.clone());
                (empty_idx, false)
            } else {
                active_lanes.push(Some(sha.clone()));
                (active_lanes.len() - 1, false)
            };

        let row_active_lanes: Vec<Option<usize>> = active_lanes
            .iter()
            .enumerate()
            .map(|(i, slot)| if slot.is_some() { Some(i) } else { None })
            .collect();

        let mut parent_lanes = Vec::new();
        if dto.parents.is_empty() {
            active_lanes[lane_idx] = None;
        } else {
            let p0 = &dto.parents[0];
            active_lanes[lane_idx] = Some(p0.clone());
            parent_lanes.push(lane_idx);

            for p in &dto.parents[1..] {
                if let Some(existing_lane) =
                    active_lanes.iter().position(|l| l.as_deref() == Some(p))
                {
                    parent_lanes.push(existing_lane);
                } else if let Some(empty_idx) = active_lanes.iter().position(|l| l.is_none()) {
                    active_lanes[empty_idx] = Some(p.clone());
                    parent_lanes.push(empty_idx);
                } else {
                    active_lanes.push(Some(p.clone()));
                    parent_lanes.push(active_lanes.len() - 1);
                }
            }
        }

        while active_lanes.last() == Some(&None) {
            active_lanes.pop();
        }

        let mut refs = Vec::new();
        for r in &dto.refs {
            let r = r.trim();
            if r.starts_with("HEAD -> ") {
                refs.push(crate::work::GitGraphRef {
                    name: r.trim_start_matches("HEAD -> ").to_string(),
                    kind: crate::work::GitGraphRefKind::Head,
                    is_head: true,
                });
            } else if r == "HEAD" {
                refs.push(crate::work::GitGraphRef {
                    name: "HEAD".to_string(),
                    kind: crate::work::GitGraphRefKind::Head,
                    is_head: true,
                });
            } else if let Some(tag) = r.strip_prefix("tag: ") {
                refs.push(crate::work::GitGraphRef {
                    name: tag.to_string(),
                    kind: crate::work::GitGraphRefKind::Tag,
                    is_head: false,
                });
            } else if r.contains('/') {
                refs.push(crate::work::GitGraphRef {
                    name: r.to_string(),
                    kind: crate::work::GitGraphRefKind::RemoteBranch,
                    is_head: false,
                });
            } else {
                refs.push(crate::work::GitGraphRef {
                    name: r.to_string(),
                    kind: crate::work::GitGraphRefKind::LocalBranch,
                    is_head: false,
                });
            }
        }

        let short_sha = if sha.len() >= 7 {
            sha[..7].to_string()
        } else {
            sha.clone()
        };

        result.push(crate::work::GitGraphCommit {
            sha: dto.sha,
            short_sha,
            parents: dto.parents,
            author_name: dto.author_name,
            author_email: dto.author_email,
            timestamp: dto.timestamp,
            refs,
            subject: dto.subject,
            lane: lane_idx,
            active_lanes: row_active_lanes,
            parent_lanes,
            has_incoming,
        });
    }

    result
}
