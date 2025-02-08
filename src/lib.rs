

pub use git2::{DiffFormat, DiffOptions, Repository, Signature, Sort, Status, StatusOptions};
use log::info;



use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use anyhow::Result;

#[derive(Debug, Clone)]
pub struct FileStatus {
    pub path: PathBuf,
    pub status: Status,
}

pub struct RepoCache {
    pub repo: Arc<Mutex<Repository>>,
    pub statuses: Arc<Mutex<Vec<FileStatus>>>,
}

impl RepoCache {
    pub fn open(path: &Path) -> Result<Self> {
        let repo = Repository::open(path)?;
        Ok(Self {
            repo: Arc::new(Mutex::new(repo)),
            statuses: Arc::new(Mutex::new(vec![])),
        })
    }

    pub fn stage(&self, path: &Path) -> Result<()> {
        let mut index = self.repo.lock().unwrap().index()?;
        index.add_path(path)?;
        index.write()?;
        self.get_status()?;
        Ok(())
    }

    pub fn unstage(&self, path: &Path) -> Result<()> {
        let mut index = self.repo.lock().unwrap().index()?;
        index.remove_path(path)?;
        index.write()?;
        self.get_status()?;
        Ok(())
    }

    pub fn get_log(&self) -> Result<()> {
        let repo = self.repo.lock().unwrap();

        // 2. Get the HEAD reference and extract its OID (the commit hash)
        let head = repo.head()?;
        let head_oid = head.target().ok_or_else(|| {
            git2::Error::from_str("HEAD reference was not a direct commit (detached HEAD?)")
        })?;

        // 3. Create a RevWalk, push HEAD, and choose sorting
        let mut revwalk = repo.revwalk()?;
        revwalk.push(head_oid)?;
        revwalk.set_sorting(Sort::TIME | Sort::REVERSE)?; // Oldest to newest by commit time

        // 4. Iterate over each commit OID in the RevWalk
        for oid_result in revwalk {
            let oid = oid_result?;
            let commit = repo.find_commit(oid)?;

            // Retrieve commit metadata
            let author = commit.author();
            let name = author.name().unwrap_or("Unknown");
            let email = author.email().unwrap_or("unknown@example.com");
            let timestamp = commit.time().seconds(); // Unix timestamp
            let message = commit.message().unwrap_or("<no commit message>");

            // Print information (roughly like `git log`)
            println!("commit {}", commit.id());
            println!("Author: {} <{}>", name, email);
            // Convert the timestamp if you want a human-readable date
            println!("Date:   {}", timestamp);
            println!();
            println!("    {}", message);
            println!();
        }

        Ok(())
    }

    pub fn commit(&self) -> Result<()> {
        // Open the repository in the current directory

        let repo = self.repo.lock().unwrap();

        // Read the repo’s config
        let config = repo.config()?;

        // Retrieve the user.name and user.email from the config
        let name = config.get_string("user.name")?;
        let email = config.get_string("user.email")?;

        // 1. Get the current HEAD commit (the parent for our new commit)
        //    and extract the reference
        let head_ref = repo.head()?;
        let parent_commit = head_ref.peel_to_commit()?;

        // 2. Access the index (which contains the files you've staged)
        let mut index = repo.index()?;

        // 3. Write the index to a tree and get the resulting tree ID
        let tree_id = index.write_tree()?;

        // 4. Find the tree object in the repo
        let tree = repo.find_tree(tree_id)?;

        // 5. Create a commit signature (author and committer)
        //    You can customize name/email or pull from repo config
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

        println!("New commit created: {}", commit_id);

        _ = self.get_status();

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
            print!("{}", String::from_utf8_lossy(line.content()));

            result.push_str(&String::from_utf8_lossy(line.content()));

            // Returning `true` means "keep processing"
            true
        })?;

        Ok(result)
    }

    /// Like git status. Caches the result internally
    /// so you can quickly access it again through Repository.statuses
    /// This function is threaded and does not return anything.
    pub fn get_status(&self) -> Result<()> {
        let repo = self.repo.clone();
        let r_statuses = self.statuses.clone();

        std::thread::spawn(move || {
            let mut status_opts = StatusOptions::new();
            status_opts
                .include_untracked(true) // Show untracked files
                .recurse_untracked_dirs(true); // Show untracked files within dirs

            // Get the status of all files in the repo
            // let statuses = repo.lock().unwrap().statuses(Some(&mut status_opts)).unwrap();
            let binding = repo.lock().unwrap();
            let statuses = binding.statuses(Some(&mut status_opts)).unwrap();

            // Iterate through each file's status
            r_statuses.lock().unwrap().clear();
            for entry in statuses.iter() {
                let path = entry.path().unwrap_or("<none>");
                info!("{path}");
                // You can check various bits in `status`:
                // - INDEX_* for staged changes
                // - WT_* for working tree changes (untracked, modified, etc.)
                r_statuses.lock().unwrap().push(FileStatus {
                    path: PathBuf::from(path),
                    status: entry.status(),
                });
            }
        });

        Ok(())
    }
}