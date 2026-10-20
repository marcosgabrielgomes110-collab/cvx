use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use cvx_index::{Index, IndexEntry, MODE_FILE};
use cvx_object::ObjectId;

fn index(entries: usize) -> Index {
    let mut index = Index::new();
    for number in 0..entries {
        index
            .insert(IndexEntry::new(
                format!("src/module-{:03}/file-{number:06}.rs", number % 100).into_bytes(),
                ObjectId::from_bytes([(number % 251) as u8; 20]),
                MODE_FILE,
            ))
            .expect("insert");
    }
    index
}

fn bench_index(c: &mut Criterion) {
    let mut group = c.benchmark_group("index");
    group.sample_size(20);

    for entries in [1000, 10_000, 50_000] {
        let index = index(entries);
        let bytes = index.serialize().expect("serialize");

        group.bench_with_input(
            BenchmarkId::new("serialize", entries),
            &index,
            |bencher, index| {
                bencher.iter(|| black_box(index.serialize()).expect("serialize"));
            },
        );
        group.bench_with_input(
            BenchmarkId::new("parse", entries),
            &bytes,
            |bencher, bytes| {
                bencher.iter(|| black_box(Index::parse(black_box(bytes))).expect("parse"));
            },
        );
    }
    group.finish();
}

criterion_group!(benches, bench_index);
criterion_main!(benches);
