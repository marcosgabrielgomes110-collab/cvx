use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};

use cvx_core::{Error, IoResultExt, Result};
use cvx_index::IndexLock;
use cvx_object::ObjectId;
use cvx_pack::{PackBase, PackInput};
use cvx_transport::{NetworkTransport, PushCommand, looks_like_scp};

use crate::init::{InitOptions, InitStorage};
use crate::repository::{CVX_DIR, GIT_DIR, Repository};

const DEFAULT_REMOTE: &str = "origin";

/// A configured remote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteInfo {
    /// Remote name.
    pub name: String,
    /// Remote URL or local path.
    pub url: String,
    /// Fetch refspec, when configured.
    pub fetch: Option<String>,
}

/// Options for `cvx clone`.
#[derive(Debug, Clone)]
pub struct CloneOptions {
    /// Source URL or local path.
    pub url: String,
    /// Destination directory.
    pub directory: PathBuf,
    /// Branch to check out. Defaults to the remote's current branch.
    pub branch: Option<String>,
    /// Control-directory convention.
    pub storage: InitStorage,
    /// Shallow history depth.
    pub depth: Option<u32>,
    /// Partial-clone filter, such as `blob:none`.
    pub filter: Option<String>,
    /// Create a bare destination without a worktree.
    pub bare: bool,
}

/// Result of a clone.
#[derive(Debug, Clone)]
pub struct CloneOutcome {
    /// Opened destination repository.
    pub repository: Repository,
    /// Checked-out branch.
    pub branch: String,
    /// Commit at `HEAD`.
    pub head: ObjectId,
    /// Number of objects copied.
    pub object_count: usize,
    /// Remote URL stored in the destination configuration.
    pub remote: String,
}

/// Result of a fetch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchOutcome {
    /// Remote name.
    pub remote: String,
    /// Remote-tracking references updated.
    pub updated: Vec<(String, ObjectId)>,
    /// Objects copied into the local object database.
    pub objects: usize,
    /// Tags copied.
    pub tags: usize,
}

/// Options for `cvx push`.
#[derive(Debug, Clone)]
pub struct PushOptions {
    /// Remote name. Defaults to `origin`.
    pub remote: Option<String>,
    /// Branch to push. Defaults to the current branch.
    pub branch: Option<String>,
    /// Allow non-fast-forward updates.
    pub force: bool,
    /// Require the remote value to match the local remote-tracking ref.
    pub force_with_lease: bool,
}

/// Result of a push.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushOutcome {
    /// Remote name.
    pub remote: String,
    /// Branch name.
    pub branch: String,
    /// Previous remote tip, when the branch existed.
    pub old: Option<ObjectId>,
    /// New remote tip.
    pub new: ObjectId,
    /// Objects copied to the remote.
    pub objects: usize,
}

impl Repository {
    /// Lists configured remotes in name order.
    pub fn remotes(&self) -> Result<Vec<RemoteInfo>> {
        let config = self.config()?;
        let mut remotes: BTreeMap<String, RemoteInfo> = BTreeMap::new();

        for entry in config.entries() {
            if !entry.section.eq_ignore_ascii_case("remote") {
                continue;
            }
            let Some(name) = &entry.subsection else {
                continue;
            };
            let remote = remotes.entry(name.clone()).or_insert_with(|| RemoteInfo {
                name: name.clone(),
                url: String::new(),
                fetch: None,
            });
            match entry.name.as_str() {
                "url" => remote.url = entry.value.clone(),
                "fetch" => remote.fetch = Some(entry.value.clone()),
                _ => {}
            }
        }

        Ok(remotes
            .into_values()
            .filter(|remote| !remote.url.is_empty())
            .collect())
    }

    /// Adds a remote configuration entry.
    pub fn add_remote(&self, name: &str, url: &str) -> Result<RemoteInfo> {
        validate_remote_name(name)?;
        if self.remote_url(name)?.is_some() {
            return Err(Error::Message(format!("remote '{name}' already exists")));
        }
        let url = normalize_remote_url(url)?;
        let text = format!(
            "\n[remote \"{name}\"]\n\turl = {}\n\tfetch = +refs/heads/*:refs/remotes/{name}/*\n",
            config_value(&url)
        );
        self.append_config(&text)?;
        Ok(RemoteInfo {
            name: name.to_owned(),
            url,
            fetch: Some(format!("+refs/heads/*:refs/remotes/{name}/*")),
        })
    }

    /// Updates a remote's URL, adding the remote when necessary.
    pub fn set_remote_url(&self, name: &str, url: &str) -> Result<()> {
        validate_remote_name(name)?;
        let url = normalize_remote_url(url)?;
        if self.remote_url(name)?.is_some() {
            self.append_config(&format!(
                "\n[remote \"{name}\"]\n\turl = {}\n",
                config_value(&url)
            ))?;
        } else {
            self.add_remote(name, &url)?;
        }
        Ok(())
    }

    /// Returns a remote's configured URL.
    pub fn remote_url(&self, name: &str) -> Result<Option<String>> {
        Ok(self
            .config()?
            .get(&format!("remote.{name}.url"))
            .map(str::to_owned))
    }

    /// Clones a repository into a new directory.
    pub fn clone_local(options: &CloneOptions) -> Result<CloneOutcome> {
        if is_network_url(&options.url) {
            return Self::clone_http(options);
        }
        if options.depth.is_some() || options.filter.is_some() {
            return Err(Error::unsupported(
                "shallow and filtered clones from local paths",
            ));
        }
        let source = open_local_remote(&options.url)?;
        let source_url = if source.is_bare() {
            source.control_dir().to_string_lossy().into_owned()
        } else {
            source
                .control_dir()
                .parent()
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_else(|| options.url.clone())
        };

        if options.directory.exists()
            && std::fs::read_dir(&options.directory)
                .map_err(|error| Error::io(options.directory.display().to_string(), error))?
                .next()
                .is_some()
        {
            return Err(Error::Message(format!(
                "destination '{}' already exists and is not empty",
                options.directory.display()
            )));
        }

        let heads = source.refs().list_refs("refs/heads")?;
        if heads.is_empty() {
            return Err(Error::Message(
                "cannot clone a repository without commits".to_owned(),
            ));
        }
        let tags = source.refs().list_refs("refs/tags")?;

        let source_branch = source.branch_name()?;
        let branch = match &options.branch {
            Some(branch) => branch.clone(),
            None => source_branch
                .filter(|branch| heads.contains_key(&format!("refs/heads/{branch}")))
                .or_else(|| {
                    heads
                        .keys()
                        .next()
                        .and_then(|reference| reference.strip_prefix("refs/heads/"))
                        .map(str::to_owned)
                })
                .ok_or_else(|| Error::Message("remote has no branches".to_owned()))?,
        };
        let head = *heads
            .get(&format!("refs/heads/{branch}"))
            .ok_or_else(|| Error::Message(format!("remote branch '{branch}' does not exist")))?;

        let mut roots: Vec<ObjectId> = heads.values().copied().collect();
        roots.extend(tags.values().copied());
        let objects = source.reachable_objects(&roots)?;

        let init = Repository::init(InitOptions {
            directory: options.directory.clone(),
            initial_branch: Some(branch.clone()),
            storage: options.storage,
            object_format: source.hash_algorithm(),
            bare: options.bare,
            quiet: true,
        })?;
        let repository = match init {
            crate::InitOutcome::Created(repository)
            | crate::InitOutcome::Reinitialized(repository)
            | crate::InitOutcome::ExistingGit(repository) => repository,
        };

        let mut contents = Vec::with_capacity(objects.len());
        for oid in &objects {
            let object = source.read_object(*oid)?;
            contents.push((object.kind, object.data));
        }
        let inputs: Vec<PackInput<'_>> = contents
            .iter()
            .map(|(kind, data)| PackInput { kind: *kind, data })
            .collect();
        cvx_pack::write_pack_with_algorithm(
            repository.control_dir().join("objects/pack"),
            &inputs,
            repository.hash_algorithm(),
        )?;

        let repository = Repository::open(repository.control_dir())?;
        finish_clone(
            repository,
            &heads,
            &tags,
            &branch,
            head,
            objects.len(),
            &source_url,
        )
    }

    fn clone_http(options: &CloneOptions) -> Result<CloneOutcome> {
        ensure_empty_destination(&options.directory)?;
        let transport = NetworkTransport::open(&options.url)?;
        let algorithm = transport.object_format()?;
        let refs =
            transport.ls_refs_with_algorithm(&["HEAD", "refs/heads/", "refs/tags/"], algorithm)?;

        let mut heads = BTreeMap::new();
        let mut tags = BTreeMap::new();
        for reference in &refs.refs {
            if reference.name.starts_with("refs/heads/") {
                heads.insert(reference.name.clone(), reference.oid);
            } else if reference.name.starts_with("refs/tags/") {
                tags.insert(reference.name.clone(), reference.oid);
            }
        }
        if heads.is_empty() {
            return Err(Error::Message(
                "cannot clone a repository without commits".to_owned(),
            ));
        }

        let branch = match &options.branch {
            Some(branch) => branch.clone(),
            None => refs
                .head_symref
                .as_deref()
                .and_then(|target| target.strip_prefix("refs/heads/"))
                .filter(|branch| heads.contains_key(&format!("refs/heads/{branch}")))
                .map(str::to_owned)
                .or_else(|| {
                    heads
                        .keys()
                        .next()
                        .and_then(|reference| reference.strip_prefix("refs/heads/"))
                        .map(str::to_owned)
                })
                .ok_or_else(|| Error::Message("remote has no branches".to_owned()))?,
        };
        let head = *heads
            .get(&format!("refs/heads/{branch}"))
            .ok_or_else(|| Error::Message(format!("remote branch '{branch}' does not exist")))?;

        let mut wants: Vec<ObjectId> = heads.values().copied().collect();
        wants.extend(tags.values().copied());
        for reference in &refs.refs {
            if let Some(peeled) = reference.peeled {
                wants.push(peeled);
            }
        }
        wants.sort_unstable();
        wants.dedup();

        for oid in heads.values().chain(tags.values()) {
            if oid.algorithm() != algorithm {
                return Err(Error::Message(
                    "the remote advertised mixed object formats".to_owned(),
                ));
            }
        }
        let response = transport.fetch_full_to_file(
            &wants,
            options.depth,
            &[],
            options.filter.as_deref(),
            algorithm,
            false,
        )?;
        let pack = read_pack_file(&response.pack_path)?;
        let objects = cvx_pack::ingest_pack_with_bases(&pack, algorithm, |_| Ok(None))?;

        let init = Repository::init(InitOptions {
            directory: options.directory.clone(),
            initial_branch: Some(branch.clone()),
            storage: options.storage,
            object_format: algorithm,
            bare: options.bare,
            quiet: true,
        })?;
        let repository = match init {
            crate::InitOutcome::Created(repository)
            | crate::InitOutcome::Reinitialized(repository)
            | crate::InitOutcome::ExistingGit(repository) => repository,
        };
        for object in &objects {
            repository.objects().write(object.kind, &object.data)?;
        }
        repository.write_shallow(&response.shallow)?;
        if options.filter.is_some() {
            repository.append_config(&format!("\n[cvx]\n\tpartialclone = {DEFAULT_REMOTE}\n"))?;
        }

        finish_clone(
            repository,
            &heads,
            &tags,
            &branch,
            head,
            objects.len(),
            transport.base_url(),
        )
    }

    /// Fetches branches and tags from a configured local remote.
    pub fn fetch(&self, remote: Option<&str>) -> Result<FetchOutcome> {
        let remote_name = remote.unwrap_or(DEFAULT_REMOTE);
        let url = self
            .remote_url(remote_name)?
            .ok_or_else(|| Error::Message(format!("remote '{remote_name}' is not configured")))?;
        if is_network_url(&url) {
            return self.fetch_http(remote_name, &url);
        }
        let source = open_local_remote(&url)?;
        if source.hash_algorithm() != self.hash_algorithm() {
            return Err(Error::Message(
                "cannot fetch between repositories with different object formats".to_owned(),
            ));
        }

        let heads = source.refs().list_refs("refs/heads")?;
        let tags = source.refs().list_refs("refs/tags")?;
        let mut roots: Vec<ObjectId> = heads.values().copied().collect();
        roots.extend(tags.values().copied());
        let objects = source.reachable_objects(&roots)?;

        let mut copied = 0;
        for oid in &objects {
            if !self.objects().contains(*oid)? {
                let object = source.read_object(*oid)?;
                self.objects().write(object.kind, &object.data)?;
                copied += 1;
            }
        }

        let mut updated = Vec::new();
        for (reference, oid) in &heads {
            let Some(name) = reference.strip_prefix("refs/heads/") else {
                continue;
            };
            let tracking = format!("refs/remotes/{remote_name}/{name}");
            let previous = self.refs().read_ref(&tracking)?;
            self.refs().update_ref(&tracking, *oid, previous)?;
            updated.push((tracking, *oid));
        }

        let mut copied_tags = 0;
        for (reference, oid) in &tags {
            let Some(name) = reference.strip_prefix("refs/tags/") else {
                continue;
            };
            if self
                .refs()
                .read_ref(&format!("refs/tags/{name}"))?
                .is_none()
            {
                self.refs()
                    .update_ref(&format!("refs/tags/{name}"), *oid, None)?;
                copied_tags += 1;
            }
        }

        if let Some(head) = source.branch_name()?
            && heads.contains_key(&format!("refs/heads/{head}"))
        {
            let _ = self.refs().write_ref_symbolic(
                &format!("refs/remotes/{remote_name}/HEAD"),
                &format!("refs/remotes/{remote_name}/{head}"),
            );
        }

        let mut fetch_head = String::new();
        for (reference, oid) in &heads {
            if let Some(name) = reference.strip_prefix("refs/heads/") {
                fetch_head.push_str(&format!("{oid}\t\tbranch '{name}' of {url}\n"));
            }
        }
        cvx_core::fs::write_atomic(
            &self.control_dir().join("FETCH_HEAD"),
            fetch_head.as_bytes(),
            Some(0o644),
        )?;

        Ok(FetchOutcome {
            remote: remote_name.to_owned(),
            updated,
            objects: copied,
            tags: copied_tags,
        })
    }

    fn fetch_http(&self, remote_name: &str, url: &str) -> Result<FetchOutcome> {
        let transport = NetworkTransport::open(url)?;
        let algorithm = transport.object_format()?;
        if algorithm != self.hash_algorithm() {
            return Err(Error::Message(
                "remote and local repositories use different object formats".to_owned(),
            ));
        }
        let refs =
            transport.ls_refs_with_algorithm(&["HEAD", "refs/heads/", "refs/tags/"], algorithm)?;

        let mut heads = BTreeMap::new();
        let mut tags = BTreeMap::new();
        for reference in &refs.refs {
            if reference.name.starts_with("refs/heads/") {
                heads.insert(reference.name.clone(), reference.oid);
            } else if reference.name.starts_with("refs/tags/") {
                tags.insert(reference.name.clone(), reference.oid);
            }
        }

        let mut wants: Vec<ObjectId> = heads.values().copied().collect();
        wants.extend(tags.values().copied());
        for reference in &refs.refs {
            if let Some(peeled) = reference.peeled {
                wants.push(peeled);
            }
        }
        wants.sort_unstable();
        wants.dedup();

        let mut copied = 0;
        if !wants.is_empty() {
            let response = transport.fetch_full_to_file(
                &wants,
                None,
                &[],
                None,
                self.hash_algorithm(),
                true,
            )?;
            let pack = read_pack_file(&response.pack_path)?;
            for object in self.ingest_pack_with_local_bases(&pack, self.hash_algorithm())? {
                if !self.objects().contains(object.oid)? {
                    self.objects().write(object.kind, &object.data)?;
                    copied += 1;
                }
            }
        }

        let mut updated = Vec::new();
        for (reference, oid) in &heads {
            let Some(name) = reference.strip_prefix("refs/heads/") else {
                continue;
            };
            let tracking = format!("refs/remotes/{remote_name}/{name}");
            let previous = self.refs().read_ref(&tracking)?;
            self.refs().update_ref(&tracking, *oid, previous)?;
            updated.push((tracking, *oid));
        }

        let mut copied_tags = 0;
        for (reference, oid) in &tags {
            let Some(name) = reference.strip_prefix("refs/tags/") else {
                continue;
            };
            if self
                .refs()
                .read_ref(&format!("refs/tags/{name}"))?
                .is_none()
            {
                self.refs()
                    .update_ref(&format!("refs/tags/{name}"), *oid, None)?;
                copied_tags += 1;
            }
        }

        if let Some(head) = refs
            .head_symref
            .as_deref()
            .and_then(|target| target.strip_prefix("refs/heads/"))
        {
            let _ = self.refs().write_ref_symbolic(
                &format!("refs/remotes/{remote_name}/HEAD"),
                &format!("refs/remotes/{remote_name}/{head}"),
            );
        }

        let mut fetch_head = String::new();
        for (reference, oid) in &heads {
            if let Some(name) = reference.strip_prefix("refs/heads/") {
                fetch_head.push_str(&format!(
                    "{oid}\t\tbranch '{name}' of {}\n",
                    transport.base_url()
                ));
            }
        }
        cvx_core::fs::write_atomic(
            &self.control_dir().join("FETCH_HEAD"),
            fetch_head.as_bytes(),
            Some(0o644),
        )?;

        Ok(FetchOutcome {
            remote: remote_name.to_owned(),
            updated,
            objects: copied,
            tags: copied_tags,
        })
    }

    /// Pushes a branch to a configured local remote.
    pub fn push(&self, options: &PushOptions) -> Result<PushOutcome> {
        let remote_name = options.remote.as_deref().unwrap_or(DEFAULT_REMOTE);
        let branch = match &options.branch {
            Some(branch) => branch.clone(),
            None => self.branch_name()?.ok_or_else(|| {
                Error::Message("cannot push from a detached HEAD without a branch".to_owned())
            })?,
        };
        let reference = format!("refs/heads/{branch}");
        let tip = self
            .refs()
            .read_ref(&reference)?
            .ok_or_else(|| Error::Message(format!("branch '{branch}' does not exist")))?;

        let url = self
            .remote_url(remote_name)?
            .ok_or_else(|| Error::Message(format!("remote '{remote_name}' is not configured")))?;
        if is_network_url(&url) {
            return self.push_http(options, remote_name, &branch, tip, &url);
        }
        let remote = open_local_remote(&url)?;
        if remote.hash_algorithm() != self.hash_algorithm() {
            return Err(Error::Message(
                "cannot push to a repository with a different object format".to_owned(),
            ));
        }
        let old = remote.refs().read_ref(&reference)?;
        self.check_lease(options, remote_name, &branch, old)?;
        self.run_pre_push(remote_name, &url, &reference, tip, old)?;

        let objects = self.reachable_objects(&[tip])?;
        let mut copied = 0;
        for oid in &objects {
            if !remote.objects().contains(*oid)? {
                let object = self.read_object(*oid)?;
                remote.objects().write(object.kind, &object.data)?;
                copied += 1;
            }
        }

        if let Some(old) = old
            && old != tip
            && !options.force
            && !cvx_revision::is_ancestor(old, tip, |oid| remote.read_commit(oid))?
        {
            return Err(Error::Message(format!(
                "updates were rejected because '{branch}' is not a fast-forward; use --force"
            )));
        }

        remote.refs().update_ref(&reference, tip, old)?;

        if remote.head()?.target() == Some(reference.as_str()) {
            let snapshot = remote.tree_snapshot_for_commit(tip)?;
            let filemode = remote.filemode()?;
            let index = remote.read_index()?;
            let existing: BTreeMap<Vec<u8>, (u32, ObjectId)> = index
                .entries()
                .iter()
                .filter(|entry| entry.stage == 0)
                .map(|entry| (entry.path.clone(), (entry.mode, entry.oid)))
                .collect();
            remote.checkout_target(&snapshot, &existing, filemode, true)?;
            let new_index = remote.index_from_tree(&snapshot, filemode)?;
            let lock = IndexLock::acquire(remote.control_dir())?;
            lock.commit(&new_index)?;
        }

        Ok(PushOutcome {
            remote: remote_name.to_owned(),
            branch,
            old,
            new: tip,
            objects: copied,
        })
    }

    fn push_http(
        &self,
        options: &PushOptions,
        remote_name: &str,
        branch: &str,
        tip: ObjectId,
        url: &str,
    ) -> Result<PushOutcome> {
        let transport = NetworkTransport::open(url)?;
        let advertisement = transport.discover_receive_pack()?;
        let reference = format!("refs/heads/{branch}");
        let old = advertisement
            .refs
            .iter()
            .find(|remote| remote.name == reference)
            .map(|remote| remote.oid);

        self.check_lease(options, remote_name, branch, old)?;
        self.run_pre_push(remote_name, url, &reference, tip, old)?;
        if let Some(old) = old
            && old != tip
            && !options.force
            && self.objects().contains(old)?
            && !cvx_revision::is_ancestor(old, tip, |oid| self.read_commit(oid))?
        {
            return Err(Error::Message(format!(
                "updates were rejected because '{branch}' is not a fast-forward; use --force"
            )));
        }

        let mut objects = self.reachable_objects(&[tip])?;
        let mut base_data: Vec<(cvx_object::ObjectKind, ObjectId, Vec<u8>)> = Vec::new();
        if advertisement.capabilities.contains_key("thin-pack")
            && let Some(old) = old
            && self.objects().contains(old)?
        {
            let remote_objects = self.reachable_objects(&[old])?;
            let remote_set: BTreeSet<ObjectId> = remote_objects.iter().copied().collect();
            objects.retain(|oid| !remote_set.contains(oid));
            let mut budget = 32 * 1024 * 1024usize;
            for oid in remote_objects.iter().take(256) {
                let object = self.read_object(*oid)?;
                if object.data.len() > 8 * 1024 * 1024 || object.data.len() > budget {
                    continue;
                }
                budget -= object.data.len();
                base_data.push((object.kind, *oid, object.data));
            }
        }

        let mut contents = Vec::with_capacity(objects.len());
        for oid in &objects {
            let object = self.read_object(*oid)?;
            contents.push((object.kind, object.data));
        }
        let inputs: Vec<PackInput<'_>> = contents
            .iter()
            .map(|(kind, data)| PackInput { kind: *kind, data })
            .collect();
        let bases: Vec<PackBase<'_>> = base_data
            .iter()
            .map(|(kind, oid, data)| PackBase {
                oid: *oid,
                kind: *kind,
                data,
            })
            .collect();
        let pack = if bases.is_empty() {
            cvx_pack::encode_pack_with_algorithm(&inputs, self.hash_algorithm())?
        } else {
            cvx_pack::encode_pack_thin_with_algorithm(&inputs, &bases, self.hash_algorithm())?
        };

        transport.push(
            &[PushCommand {
                old,
                new: tip,
                reference,
            }],
            &pack,
        )?;

        Ok(PushOutcome {
            remote: remote_name.to_owned(),
            branch: branch.to_owned(),
            old,
            new: tip,
            objects: objects.len(),
        })
    }

    fn check_lease(
        &self,
        options: &PushOptions,
        remote_name: &str,
        branch: &str,
        remote_old: Option<ObjectId>,
    ) -> Result<()> {
        if !options.force_with_lease {
            return Ok(());
        }
        let tracking = format!("refs/remotes/{remote_name}/{branch}");
        let expected = self.refs().read_ref(&tracking)?;
        if expected != remote_old {
            return Err(Error::Message(format!(
                "push rejected: remote '{branch}' changed; fetch and retry"
            )));
        }
        Ok(())
    }

    fn run_pre_push(
        &self,
        remote_name: &str,
        url: &str,
        reference: &str,
        tip: ObjectId,
        old: Option<ObjectId>,
    ) -> Result<()> {
        let zero = ObjectId::zero(self.hash_algorithm());
        let input = format!("{reference} {tip} {reference} {}\n", old.unwrap_or(zero));
        self.run_hook(
            "pre-push",
            &[remote_name.to_owned(), url.to_owned()],
            Some(input.as_bytes()),
        )
    }

    fn append_config(&self, text: &str) -> Result<()> {
        let path = self.control_dir().join("config");
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .path_context(&path)?;
        file.write_all(text.as_bytes()).path_context(&path)?;
        file.flush().path_context(&path)
    }
}

fn finish_clone(
    repository: Repository,
    heads: &BTreeMap<String, ObjectId>,
    tags: &BTreeMap<String, ObjectId>,
    branch: &str,
    head: ObjectId,
    object_count: usize,
    remote_url: &str,
) -> Result<CloneOutcome> {
    let remote_name = DEFAULT_REMOTE;

    for (reference, oid) in heads {
        let Some(name) = reference.strip_prefix("refs/heads/") else {
            continue;
        };
        repository
            .refs()
            .update_ref(&format!("refs/remotes/{remote_name}/{name}"), *oid, None)?;
    }
    repository.refs().write_ref_symbolic(
        &format!("refs/remotes/{remote_name}/HEAD"),
        &format!("refs/remotes/{remote_name}/{branch}"),
    )?;
    repository
        .refs()
        .update_ref(&format!("refs/heads/{branch}"), head, None)?;
    for (reference, oid) in tags {
        let Some(name) = reference.strip_prefix("refs/tags/") else {
            continue;
        };
        repository
            .refs()
            .update_ref(&format!("refs/tags/{name}"), *oid, None)?;
    }

    repository.append_config(&format!(
        "\n[remote \"{remote_name}\"]\n\turl = {}\n\tfetch = +refs/heads/*:refs/remotes/{remote_name}/*\n",
        config_value(remote_url)
    ))?;
    repository.append_config(&format!(
        "\n[branch \"{branch}\"]\n\tremote = {remote_name}\n\tmerge = refs/heads/{branch}\n"
    ))?;

    if !repository.is_bare() {
        let snapshot = repository.tree_snapshot_for_commit(head)?;
        let filemode = repository.filemode()?;
        repository.checkout_target(&snapshot, &BTreeMap::new(), filemode, true)?;
        let index = repository.index_from_tree(&snapshot, filemode)?;
        let lock = IndexLock::acquire(repository.control_dir())?;
        lock.commit(&index)?;
    }

    Ok(CloneOutcome {
        repository,
        branch: branch.to_owned(),
        head,
        object_count,
        remote: remote_url.to_owned(),
    })
}

fn ensure_empty_destination(directory: &Path) -> Result<()> {
    if directory.exists()
        && std::fs::read_dir(directory)
            .map_err(|error| Error::io(directory.display().to_string(), error))?
            .next()
            .is_some()
    {
        return Err(Error::Message(format!(
            "destination '{}' already exists and is not empty",
            directory.display()
        )));
    }
    Ok(())
}

fn read_pack_file(path: &Path) -> Result<Vec<u8>> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            let _ = std::fs::remove_file(path);
            return Err(Error::io(path.display().to_string(), error));
        }
    };
    let _ = std::fs::remove_file(path);
    Ok(bytes)
}

fn is_network_url(url: &str) -> bool {
    url.starts_with("https://")
        || url.starts_with("http://")
        || url.starts_with("ssh://")
        || looks_like_scp(url)
}

fn open_local_remote(url: &str) -> Result<Repository> {
    let path = local_path(url)?;
    let canonical = std::fs::canonicalize(&path).path_context(&path)?;
    if canonical.join(CVX_DIR).is_dir() {
        Repository::open(canonical.join(CVX_DIR))
    } else if canonical.join(GIT_DIR).is_dir() {
        Repository::open(canonical.join(GIT_DIR))
    } else {
        Repository::discover(&canonical)
    }
}

fn local_path(url: &str) -> Result<PathBuf> {
    if let Some(path) = url.strip_prefix("file://") {
        return Ok(PathBuf::from(path));
    }
    if let Some(scheme) = url.split_once("://").map(|(scheme, _)| scheme) {
        return Err(Error::unsupported(format!(
            "the '{scheme}' transport is not implemented yet"
        )));
    }
    Ok(PathBuf::from(url))
}

fn config_value(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r");
    format!("\"{escaped}\"")
}

fn normalize_remote_url(url: &str) -> Result<String> {
    if url.contains("://") {
        return Ok(url.to_owned());
    }
    let path = Path::new(url);
    if path.exists() {
        return Ok(std::fs::canonicalize(path)
            .path_context(path)?
            .to_string_lossy()
            .into_owned());
    }
    Ok(url.to_owned())
}

fn validate_remote_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.starts_with('-')
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(Error::Message(format!("invalid remote name: {name}")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_windows_paths_in_config_values() {
        let value = r"C:\repos\example";
        let text = format!("[remote \"origin\"]\n\turl = {}\n", config_value(value));
        let config = cvx_config::Config::parse_str(&text, "test").expect("config should parse");
        assert_eq!(config.get("remote.origin.url"), Some(value));
    }

    #[test]
    fn quotes_urls_with_spaces() {
        let value = "https://example.com/a b/repo.git";
        let text = format!("[remote \"origin\"]\n\turl = {}\n", config_value(value));
        let config = cvx_config::Config::parse_str(&text, "test").expect("config should parse");
        assert_eq!(config.get("remote.origin.url"), Some(value));
    }
}
