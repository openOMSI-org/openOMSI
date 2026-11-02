use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::{data_dir, level_of, xp_of, Session};

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Company {
    pub name: String,
    pub cash: f64,
    pub reputation: f64,
    pub xp: i64,
    pub completed_sessions: u64,
    processed_sessions: Vec<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct CompanyView {
    pub name: String,
    pub cash: f64,
    pub reputation: f64,
    pub xp: i64,
    pub level: i64,
    pub next_level_xp: i64,
    pub completed_sessions: u64,
    pub sessions: Vec<Session>,
}

fn path() -> PathBuf {
    data_dir().join("company.json")
}

fn load_from(p: &Path) -> Result<Company> {
    match std::fs::read_to_string(p) {
        Ok(text) => serde_json::from_str(&text).with_context(|| format!("reading {}", p.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Company::default()),
        Err(e) => Err(e).with_context(|| format!("reading {}", p.display())),
    }
}

fn save_to(p: &Path, company: &Company) -> Result<()> {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = p.with_extension(format!("json.{}.{:?}.{n}.tmp", std::process::id(), std::thread::current().id()).replace(['(', ')'], ""));
    std::fs::write(&tmp, serde_json::to_vec_pretty(company)?)?;
    std::fs::rename(&tmp, p).with_context(|| format!("writing {}", p.display()))
}

fn session_files() -> Vec<(String, Session)> {
    let dir = data_dir().join("sessions");
    let mut files: Vec<(String, Session)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let id = entry.file_name().to_string_lossy().into_owned();
            let text = std::fs::read_to_string(entry.path()).ok()?;
            Some((id, serde_json::from_str(&text).ok()?))
        })
        .collect();
    files.sort_by(|a, b| b.1.time.cmp(&a.1.time));
    files
}

fn refresh(company: &mut Company) -> bool {
    let mut changed = false;
    let mut ratings = company.reputation * company.completed_sessions as f64;
    for (id, session) in session_files() {
        if company.processed_sessions.iter().any(|seen| seen == &id) {
            continue;
        }
        company.cash += session.cash;
        company.xp += xp_of(&session);
        ratings += ((session.driving + session.comfort + session.ticketing) / 3.0).clamp(0.0, 100.0);
        company.completed_sessions += 1;
        company.processed_sessions.push(id);
        changed = true;
    }
    if changed && company.completed_sessions > 0 {
        company.reputation = (ratings / company.completed_sessions as f64).clamp(0.0, 100.0);
    }
    changed
}

fn view(company: &Company) -> CompanyView {
    let (level, next_level_xp) = level_of(company.xp);
    CompanyView {
        name: company.name.clone(),
        cash: company.cash,
        reputation: company.reputation,
        xp: company.xp,
        level,
        next_level_xp,
        completed_sessions: company.completed_sessions,
        sessions: session_files()
            .into_iter()
            .filter(|(id, _)| company.processed_sessions.iter().any(|seen| seen == id))
            .map(|(_, session)| session)
            .take(20)
            .collect(),
    }
}

pub fn get() -> Result<Option<CompanyView>> {
    let p = path();
    let mut company = load_from(&p)?;
    if company.name.trim().is_empty() {
        return Ok(None);
    }
    if refresh(&mut company) {
        save_to(&p, &company)?;
    }
    Ok(Some(view(&company)))
}

pub fn create(name: &str) -> Result<CompanyView> {
    let name = name.trim();
    if name.is_empty() {
        anyhow::bail!("company name cannot be empty");
    }
    let p = path();
    let existing = load_from(&p)?;
    if !existing.name.trim().is_empty() {
        anyhow::bail!("a company already exists");
    }
    let company = Company { name: name.to_string(), ..Company::default() };
    save_to(&p, &company)?;
    Ok(view(&company))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_company_starts_empty() {
        let company = Company { name: "Transit Co".into(), ..Company::default() };
        let result = view(&company);
        assert_eq!(result.name, "Transit Co");
        assert_eq!(result.level, 1);
        assert_eq!(result.completed_sessions, 0);
    }
}
