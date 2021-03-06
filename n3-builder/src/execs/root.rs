use std::fs;
use std::ops::Deref;
use std::path::Path;

use super::args::Args;
use super::dirs::*;
use super::program::Program;
use super::var::GlobalVars;
use crate::error::{ExecError, Result};
use crate::graph::ToValues;
use crate::n3_std::trim_path;
use crate::nodes::NodeRoot;

use glob::glob;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ExecRootConfig {
    pub create_root_dir: Option<bool>,
}

impl Default for ExecRootConfig {
    fn default() -> Self {
        Self {
            create_root_dir: None,
        }
    }
}

pub struct ExecRoot {
    node_root: NodeRoot,
    pub(super) env: GlobalVars,
}

impl Deref for ExecRoot {
    type Target = NodeRoot;

    fn deref(&self) -> &Self::Target {
        &self.node_root
    }
}

impl ExecRoot {
    pub fn try_new(env: GlobalVars, config: ExecRootConfig) -> Result<Self> {
        let n3_source_root = env.get_string(N3_SOURCE_ROOT).ok();

        let root = Self {
            node_root: NodeRoot::new(n3_source_root.as_deref()),
            env,
        };

        root.assert_root_dir(&config)?;
        root.load_local_nodes()?;

        Ok(root)
    }

    pub fn get(&mut self, name: &str) -> Result<Args> {
        let ir = self.node_root.get_exec(name)?;
        let args = ir.args();

        Ok(Args {
            root: self,
            ir,
            args,
        })
    }

    pub fn attach_env(&self, program: &mut Program) {
        program.env = Some(self.env.to_values());
    }

    fn assert_root_dir(&self, config: &ExecRootConfig) -> Result<()> {
        let path = self.env.root_dir();

        if path.exists() {
            if path.is_dir() {
                Ok(())
            } else {
                ExecError::NotDirectory { path }.into()
            }
        } else {
            match config.create_root_dir {
                Some(true) => Self::create_root_dir(&path),
                Some(false) => no_such_directory(&path),
                None => Self::ask_to_create_root_dir(&path),
            }
        }
    }

    fn load_local_nodes(&self) -> Result<()> {
        let path = self.env.root_dir().join(NODES_DIR);
        let path = path.join("**/*.n3").display().to_string();

        for path in glob(&path)? {
            let path = path?;
            let name = trim_path(&path);

            let extern_path = path.with_extension("py");
            if extern_path.exists() {
                let path_str = extern_path.display().to_string();
                self.node_root.add_extern_path(name.clone(), path_str);
            }

            let path_str = path.display().to_string();
            self.node_root.add_source_path(name, path_str);
        }
        Ok(())
    }

    #[cfg(feature = "cli")]
    fn ask_to_create_root_dir(path: &Path) -> Result<()> {
        if dialoguer::Confirm::new()
            .default(false)
            .with_prompt(format!(
                "It seems that there is no root directory on \"{}\"
- Do you want to create one?",
                path.display()
            ))
            .interact()?
        {
            Self::create_root_dir(path)
        } else {
            no_such_directory(path)
        }
    }

    #[cfg(not(feature = "cli"))]
    fn ask_to_create_root_dir(path: &Path) -> Result<()> {
        no_such_directory(path)
    }

    fn create_root_dir(path: &Path) -> Result<()> {
        fs::create_dir_all(path)?;
        for name in &[
            Path::new(DATA_DIR),
            Path::new(LOGS_DIR),
            Path::new(MODELS_DIR),
            Path::new(NODES_DIR),
            &Path::new(NODES_DIR).join(NODES_USER_DIR),
        ] {
            fs::create_dir(path.join(name))?;
        }
        Ok(())
    }
}

fn no_such_directory(path: &Path) -> Result<()> {
    ExecError::NoSuchDirectory {
        path: path.to_path_buf(),
    }
    .into()
}
