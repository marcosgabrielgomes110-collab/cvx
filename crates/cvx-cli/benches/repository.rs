use std::hint::black_box;
use std::path::{Path, PathBuf};

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use cvx_repository::{
    AddOptions, CommitOptions, DiffOptions, InitOptions, LogOptions, Repository, StatusOptions,
};

fn create_repository(files: usize) -> (tempfile::TempDir, Repository) {
    let temp = tempfile::tempdir().expect("tempdir");
    let repository = match Repository::init(InitOptions {
        directory: temp.path().to_path_buf(),
        ..InitOptions::default()
    })
    .expect("init")
    {
        cvx_repository::InitOutcome::Created(repository) => repository,
        _ => panic!("expected new repository"),
    };
    let config = repository.control_dir().join("config");
    let mut contents = std::fs::read_to_string(&config).expect("config");
    contents.push_str("\n[user]\n\tname = Bench User\n\temail = bench@example.com\n");
    std::fs::write(config, contents).expect("config");

    for number in 0..files {
        let directory = temp.path().join(format!("src/dir-{:03}", number % 100));
        std::fs::create_dir_all(&directory).expect("mkdir");
        std::fs::write(
            directory.join(format!("file-{number:06}.txt")),
            format!("file {number}\ncommon line\n"),
        )
        .expect("write");
    }

    let cwd = temp.path().to_path_buf();
    repository
        .add(AddOptions {
            paths: vec![PathBuf::from(".")],
            cwd: cwd.clone(),
            force: false,
            update: false,
            all: true,
        })
        .expect("add");
    repository
        .commit(CommitOptions {
            messages: vec!["benchmark fixture".to_owned()],
            allow_empty: false,
        })
        .expect("commit");

    (temp, repository)
}

fn bench_repository(c: &mut Criterion) {
    let mut sizes = vec![10_000usize];
    if std::env::var_os("CVX_BENCH_LARGE").is_some() {
        sizes.push(100_000);
    }
    if std::env::var_os("CVX_BENCH_1M").is_some() {
        sizes.push(1_000_000);
    }
    if let Some(value) = std::env::var_os("CVX_BENCH_FILES")
        && let Some(files) = value.to_str().and_then(|value| value.parse::<usize>().ok())
        && files > 0
        && !sizes.contains(&files)
    {
        sizes.push(files);
    }

    let mut group = c.benchmark_group("repository");
    group.sample_size(10);

    for files in sizes {
        let (temp, repository) = create_repository(files);
        let cwd = temp.path().to_path_buf();

        group.bench_with_input(BenchmarkId::new("status", files), &files, |bencher, _| {
            bencher
                .iter(|| black_box(repository.status(StatusOptions::default()).expect("status")));
        });

        let changed = Path::new("src/dir-000/file-000000.txt");
        std::fs::write(temp.path().join(changed), "changed\n").expect("write");
        group.bench_with_input(BenchmarkId::new("diff", files), &files, |bencher, _| {
            bencher.iter(|| {
                black_box(
                    repository
                        .diff(&DiffOptions {
                            cwd: cwd.clone(),
                            ..DiffOptions::default()
                        })
                        .expect("diff"),
                )
            });
        });

        group.bench_with_input(BenchmarkId::new("log", files), &files, |bencher, _| {
            bencher.iter(|| {
                black_box(
                    repository
                        .log(&LogOptions {
                            limit: Some(10),
                            ..LogOptions::default()
                        })
                        .expect("log"),
                )
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_repository);
criterion_main!(benches);
