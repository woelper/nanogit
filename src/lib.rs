pub use git2::{DiffFormat, DiffOptions, Repository, Signature, Sort, Status, StatusOptions};
use log::{debug, info};

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::SystemTime,
};

use anyhow::Result;

#[derive(Debug, Clone)]
pub struct FileStatus {
    pub path: PathBuf,
    pub status: Status,
}

#[derive(Debug, Clone)]
pub struct LogItem {
    pub name: String,
    pub email: String,
    pub commit: String,
    pub timestamp: i64,
    pub message: String,
}

pub struct RepoCache {
    pub repo: Arc<Mutex<Repository>>,
    pub statuses: Arc<Mutex<Vec<FileStatus>>>,
    pub log: Arc<Mutex<Vec<LogItem>>>,
    pub local_refresh: Arc<Mutex<Option<SystemTime>>>,
    pub remote_refresh: Arc<Mutex<Option<SystemTime>>>,
}

impl RepoCache {
    pub fn get_local_refresh(&self) -> Option<SystemTime> {
        self.local_refresh.lock().ok().map(|f| *f).flatten()
    }

    pub fn get_remote_refresh(&self) -> Option<SystemTime> {
        self.remote_refresh.lock().ok().map(|f| *f).flatten()
    }

    pub fn is_local_refreshed(&self) -> bool {
        self.local_refresh.lock().unwrap().is_some()
    }

    pub fn get_statuses(&self) -> Vec<FileStatus> {
        (*self.statuses.lock().unwrap()).clone()
    }

    pub fn get_log(&self) -> Vec<LogItem> {
        (*self.log.lock().unwrap()).clone()
    }

    pub fn get_root(&self) -> PathBuf {
        self.repo.lock().unwrap().commondir().to_path_buf()
    }

    pub fn open(path: &Path) -> Result<Self> {
        let repo = Repository::open(path)?;
        Ok(Self {
            repo: Arc::new(Mutex::new(repo)),
            statuses: Arc::new(Mutex::new(vec![])),
            log: Arc::new(Mutex::new(vec![])),
            local_refresh: Arc::new(Mutex::new(None)),
            remote_refresh: Arc::new(Mutex::new(None)),
        })
    }

    pub fn stage(&self, path: &Path) -> Result<()> {
        let mut index = self.repo.lock().unwrap().index()?;
        index.add_path(path)?;
        index.write()?;
        self.refresh()?;
        Ok(())
    }

    pub fn unstage(&self, path: &Path) -> Result<()> {
        let mut index = self.repo.lock().unwrap().index()?;
        index.remove_path(path)?;
        index.write()?;
        self.refresh()?;
        Ok(())
    }

    pub fn refresh_log(&self, max_commits: usize) -> Result<Vec<LogItem>> {
        let repo = self.repo.lock().unwrap();


        let mut revwalk = repo.revwalk()?;
        revwalk.push_head()?;
        // revwalk.set_sorting(Sort::TIME | Sort::REVERSE)?;
        
        let mut log = vec![];
        for (i, oid) in revwalk.enumerate() {
            if i >= max_commits { break; }
            let commit = repo.find_commit(oid?)?;
            let author = commit.author();
            let name = author.name().unwrap_or("Unknown").to_string();
            let email = author.email().unwrap_or("unknown@example.com").to_string();
            let timestamp = commit.time().seconds(); // Unix timestamp
            let message = commit
                .message()
                .unwrap_or("<no commit message>")
                .to_string();

            let logitem = LogItem {
                name,
                email,
                timestamp,
                message,
                commit: commit.id().to_string(),
            };

            log.push(logitem);
        }


        
        // debug!("iterate");
        // for oid_result in revwalk.take(max_commits) {
        // debug!("res");

        //     let oid = oid_result?;
        //     let commit = repo.find_commit(oid)?;

        //     // Retrieve commit metadata
           

        //     // // Print information (roughly like `git log`)
        //     // println!("commit {}", commit.id());
        //     // println!("Author: {} <{}>", name, email);
        //     // // Convert the timestamp if you want a human-readable date
        //     // println!("Date:   {}", timestamp);
        //     // println!();
        //     // println!("    {}", message);
        //     // println!();
        // }

        Ok(log)
    }

    pub fn commit(&self) -> Result<()> {
        let repo = self.repo.lock().unwrap();

        let config = repo.config()?;

        let name = config.get_string("user.name")?;
        let email = config.get_string("user.email")?;

        let head_ref = repo.head()?;
        let parent_commit = head_ref.peel_to_commit()?;

        let mut index = repo.index()?;
        let tree_id = index.write_tree()?;
        let tree = repo.find_tree(tree_id)?;
        let sig = Signature::now(&name, &email)?;
        // 6. Create the commit on HEAD, using the parent we found
        let commit_id = repo.commit(
            Some("HEAD"),          // point HEAD to our new commit
            &sig,                  // author
            &sig,                  // committer
            "Your commit message", // commit message
            &tree,                 // tree
            &[&parent_commit],     // parents
        )?;

        debug!("New commit created: {}", commit_id);

        _ = self.refresh();

        Ok(())
    }

    /// Returns a git diff for a file.
    pub fn diff(&self, path: &Path) -> Result<String> {
        let repo = self.repo.lock().unwrap();

        // Get the HEAD tree to compare against
        let head_commit = repo.head()?.peel_to_commit()?;
        let head_tree = head_commit.tree()?;

        // Build DiffOptions to target the single file
        let mut diff_opts = DiffOptions::new();

        diff_opts.minimal(true);
        diff_opts.pathspec(path);

        // 4. Generate the diff
        //    (Comparing HEAD tree to the working directory)
        let diff = repo.diff_tree_to_workdir(Some(&head_tree), Some(&mut diff_opts))?;

        // 5. Print the diff in patch format
        let mut result = String::new();

        diff.print(DiffFormat::Patch, |delta, _hunk, line| {
            // Print file header once, if desired
            // (You can check delta.is_none() to detect boundaries)
            // ...

            // Print the actual diff lines
            
            let output = format!("{} {}", line.origin(), String::from_utf8_lossy(line.content()));

            result.push_str(&output);

            // Returning `true` means "keep processing"
            true
        })?;

        Ok(result)
    }

    /// Like git status. Caches the result internally
    /// so you can quickly access it again through Repository.statuses
    /// This function is threaded and does not return anything.
    pub fn refresh(&self) -> Result<()> {
        let repo = self.repo.clone();
        let r_statuses = self.statuses.clone();
        let local_refresh = self.local_refresh.clone();

        std::thread::spawn(move || {
            let mut status_opts = StatusOptions::new();
            status_opts
                .include_untracked(true) // Show untracked files
                .recurse_untracked_dirs(true); // Show untracked files within dirs

            // Get the status of all files in the repo
            let binding = repo.lock().unwrap();
            let statuses = binding.statuses(Some(&mut status_opts)).unwrap();

            // Iterate through each file's status
            r_statuses.lock().unwrap().clear();
            for entry in statuses.iter() {
                let path = entry.path().unwrap_or("<none>");
                // debug!("{path}");
                r_statuses.lock().unwrap().push(FileStatus {
                    path: PathBuf::from(path),
                    status: entry.status(),
                });
            }
            debug!("Repository status refreshed.");
            *local_refresh.lock().unwrap() = Some(SystemTime::now());




        });

        // todo: thread

        let log = self.refresh_log(10)?;
        *self.log.lock().unwrap() = log;

        Ok(())
    }
}
