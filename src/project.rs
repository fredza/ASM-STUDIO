//! Projet ASM Studio : un point d'entrée, plusieurs sources et des includes.
//!
//! Un fichier `.asm` seul reste la façon la plus courte de commencer. Dès que
//! le programme a une routine dans un autre fichier, il lui faut toutefois un
//! endroit où dire quelle source est l'entrée et lesquelles doivent être
//! liées. `asmstudio.toml` est ce petit contrat, volontairement sans magie :
//!
//! ```toml
//! entry = "src/main.asm"
//! target = "linux"
//! sources = ["src/main.asm", "src/math.asm"]
//! includes = ["include"]
//! ```
//!
//! Le lecteur ne prétend pas être un parseur TOML général. Il accepte le
//! sous-ensemble que l'application écrit elle-même, ce qui garde le manifeste
//! inspectable par un élève sans ajouter une dépendance à l'IDE.

use std::path::{Component, Path, PathBuf};

use crate::assemble::Target;

pub const MANIFEST_NAME: &str = "asmstudio.toml";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Project {
    pub manifest: PathBuf,
    pub root: PathBuf,
    pub entry: PathBuf,
    pub sources: Vec<PathBuf>,
    pub includes: Vec<PathBuf>,
    pub target: Target,
}

impl Project {
    pub fn is_manifest(path: &Path) -> bool {
        path.file_name().is_some_and(|name| name == MANIFEST_NAME)
    }

    pub fn load(manifest: &Path) -> Result<Self, String> {
        if !Self::is_manifest(manifest) {
            return Err(format!("le manifeste doit s'appeler {MANIFEST_NAME}"));
        }
        let content = std::fs::read_to_string(manifest)
            .map_err(|e| format!("lecture de {}: {e}", manifest.display()))?;
        let root = manifest.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();
        Self::parse(&root, manifest.to_path_buf(), &content)
    }

    pub fn create(root: &Path, name: &str, target: Target, source: &str) -> Result<Self, String> {
        let root = root.join(name);
        let source_rel = PathBuf::from("src/main.asm");
        let entry = root.join(&source_rel);
        std::fs::create_dir_all(root.join("src"))
            .map_err(|e| format!("création de {}: {e}", root.display()))?;
        std::fs::create_dir_all(root.join("include"))
            .map_err(|e| format!("création de {}: {e}", root.display()))?;
        std::fs::write(&entry, source)
            .map_err(|e| format!("écriture de {}: {e}", entry.display()))?;

        let project = Self {
            manifest: root.join(MANIFEST_NAME),
            root: root.clone(),
            entry: entry.clone(),
            sources: vec![entry.clone()],
            includes: vec![root.join("include")],
            target,
        };
        std::fs::write(&project.manifest, project.content())
            .map_err(|e| format!("écriture de {}: {e}", project.manifest.display()))?;
        Ok(project)
    }

    pub fn content(&self) -> String {
        fn quoted(path: &Path) -> String {
            format!("\"{}\"", path.display().to_string().replace('\\', "\\\\").replace('"', "\\\""))
        }
        fn rel<'a>(root: &Path, path: &'a Path) -> &'a Path {
            path.strip_prefix(root).unwrap_or(path)
        }
        let array = |paths: &[PathBuf]| {
            paths
                .iter()
                .map(|p| quoted(rel(&self.root, p)))
                .collect::<Vec<_>>()
                .join(", ")
        };
        format!(
            "# Projet ASM Studio\nentry = {}\ntarget = \"{}\"\nsources = [{}]\nincludes = [{}]\n",
            quoted(rel(&self.root, &self.entry)),
            self.target.key(),
            array(&self.sources),
            array(&self.includes),
        )
    }

    /// Répertoires passés à NASM. Chaque source peut inclure un voisin, en
    /// plus des dossiers explicitement déclarés dans le manifeste.
    pub fn include_dirs(&self) -> Vec<PathBuf> {
        let mut dirs = self.includes.clone();
        for source in &self.sources {
            if let Some(parent) = source.parent()
                && !dirs.contains(&parent.to_path_buf())
            {
                dirs.push(parent.to_path_buf());
            }
        }
        dirs
    }

    fn parse(root: &Path, manifest: PathBuf, content: &str) -> Result<Self, String> {
        let mut entry = None;
        let mut target = Target::Linux;
        let mut sources = None;
        let mut includes = Vec::new();
        for (line_no, raw) in content.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                return Err(format!("ligne {} : clé = valeur attendue", line_no + 1));
            };
            match key.trim() {
                "entry" => entry = Some(local_path(parse_string(value.trim(), line_no + 1)?, line_no + 1)?),
                "target" => target = Target::from_key(&parse_string(value.trim(), line_no + 1)?),
                "sources" => sources = Some(parse_paths(value.trim(), line_no + 1)?),
                "includes" => includes = parse_paths(value.trim(), line_no + 1)?,
                other => return Err(format!("ligne {} : clé inconnue « {other} »", line_no + 1)),
            }
        }
        let entry = entry.ok_or_else(|| "entry est obligatoire".to_string())?;
        let mut sources = sources.unwrap_or_else(|| vec![entry.clone()]);
        if sources.is_empty() {
            return Err("sources ne peut pas être vide".to_string());
        }
        if !sources.contains(&entry) {
            sources.insert(0, entry.clone());
        }
        Ok(Self {
            manifest,
            root: root.to_path_buf(),
            entry: root.join(entry),
            sources: sources.into_iter().map(|p| root.join(p)).collect(),
            includes: includes.into_iter().map(|p| root.join(p)).collect(),
            target,
        })
    }
}

fn parse_string(value: &str, line: usize) -> Result<String, String> {
    let value = value.trim();
    if !(value.starts_with('"') && value.ends_with('"') && value.len() >= 2) {
        return Err(format!("ligne {line} : chaîne entre guillemets attendue"));
    }
    let inner = &value[1..value.len() - 1];
    let mut out = String::new();
    let mut escaped = false;
    for c in inner.chars() {
        if escaped {
            match c {
                '\\' | '"' => out.push(c),
                _ => return Err(format!("ligne {line} : échappement \\{c} inconnu")),
            }
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else {
            out.push(c);
        }
    }
    if escaped {
        return Err(format!("ligne {line} : échappement incomplet"));
    }
    Ok(out)
}

fn parse_paths(value: &str, line: usize) -> Result<Vec<PathBuf>, String> {
    let value = value.trim();
    if !(value.starts_with('[') && value.ends_with(']')) {
        return Err(format!("ligne {line} : tableau de chaînes attendu"));
    }
    let body = value[1..value.len() - 1].trim();
    if body.is_empty() {
        return Ok(Vec::new());
    }
    body.split(',')
        .map(|s| local_path(parse_string(s.trim(), line)?, line))
        .collect()
}

fn local_path(text: String, line: usize) -> Result<PathBuf, String> {
    let path = PathBuf::from(text);
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path.components().any(|c| matches!(c, Component::ParentDir | Component::RootDir | Component::Prefix(_)))
    {
        return Err(format!("ligne {line} : le chemin doit rester dans le projet"));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_manifest_loads_sources_in_the_project_only() {
        let project = Project::parse(
            Path::new("/tmp/demo"),
            PathBuf::from("/tmp/demo/asmstudio.toml"),
            "entry = \"src/main.asm\"\ntarget = \"linux\"\nsources = [\"src/main.asm\", \"src/math.asm\"]\nincludes = [\"include\"]\n",
        )
        .expect("manifest valide");
        assert_eq!(project.entry, PathBuf::from("/tmp/demo/src/main.asm"));
        assert_eq!(project.sources.len(), 2);
        assert_eq!(project.includes, vec![PathBuf::from("/tmp/demo/include")]);
    }

    #[test]
    fn an_escape_from_the_project_is_refused() {
        let err = Project::parse(
            Path::new("/tmp/demo"),
            PathBuf::from("/tmp/demo/asmstudio.toml"),
            "entry = \"../elsewhere.asm\"\n",
        )
        .expect_err("le parent doit être refusé");
        assert!(err.contains("rester dans le projet"));
    }

    #[test]
    fn written_content_can_be_read_again() {
        let p = Project {
            manifest: PathBuf::from("/tmp/demo/asmstudio.toml"),
            root: PathBuf::from("/tmp/demo"),
            entry: PathBuf::from("/tmp/demo/src/main.asm"),
            sources: vec![PathBuf::from("/tmp/demo/src/main.asm"), PathBuf::from("/tmp/demo/src/math.asm")],
            includes: vec![PathBuf::from("/tmp/demo/include")],
            target: Target::Linux,
        };
        let reread = Project::parse(&p.root, p.manifest.clone(), &p.content()).expect("aller-retour");
        assert_eq!(reread, p);
    }
}
