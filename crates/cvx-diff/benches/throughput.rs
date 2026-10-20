use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use cvx_diff::{DiffFile, DiffOptions, DiffSide, render};
use cvx_object::{MODE_FILE, ObjectId};

fn side(content: Vec<u8>) -> DiffSide {
    DiffSide {
        mode: MODE_FILE,
        oid: ObjectId::from_bytes([1; 20]),
        content,
    }
}

fn bench_diff(c: &mut Criterion) {
    let mut group = c.benchmark_group("diff");
    group.sample_size(20);

    for lines in [1000, 10_000, 50_000] {
        let old: Vec<u8> = (0..lines)
            .map(|line| format!("line {line} original\n"))
            .collect::<String>()
            .into_bytes();
        let new: Vec<u8> = (0..lines)
            .map(|line| {
                if line % 10 == 0 {
                    format!("line {line} changed\n")
                } else {
                    format!("line {line} original\n")
                }
            })
            .collect::<String>()
            .into_bytes();
        let file = DiffFile {
            path: b"large.txt".to_vec(),
            old_path: None,
            similarity: None,
            old: Some(side(old)),
            new: Some(side(new)),
        };

        group.bench_with_input(BenchmarkId::new("render", lines), &file, |bencher, file| {
            bencher.iter(|| {
                black_box(render(
                    black_box(std::slice::from_ref(file)),
                    &DiffOptions::default(),
                ))
            });
        });
        group.bench_with_input(BenchmarkId::new("stat", lines), &file, |bencher, file| {
            bencher.iter(|| {
                black_box(render(
                    black_box(std::slice::from_ref(file)),
                    &DiffOptions {
                        stat: true,
                        ..DiffOptions::default()
                    },
                ))
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_diff);
criterion_main!(benches);
