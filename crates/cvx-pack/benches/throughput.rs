use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use cvx_object::ObjectKind;
use cvx_pack::{PackInput, ingest_pack, write_pack};

fn objects(count: usize, size: usize) -> Vec<(ObjectKind, Vec<u8>)> {
    (0..count)
        .map(|index| {
            let mut data = format!("object {index}\n").into_bytes();
            data.resize(size, b'x');
            (ObjectKind::Blob, data)
        })
        .collect()
}

fn bench_pack(c: &mut Criterion) {
    let mut group = c.benchmark_group("pack");
    group.sample_size(20);

    for (count, size) in [(100, 256), (1000, 1024), (5000, 256)] {
        let data = objects(count, size);
        let inputs: Vec<PackInput<'_>> = data
            .iter()
            .map(|(kind, data)| PackInput { kind: *kind, data })
            .collect();
        let temp = tempfile::tempdir().expect("tempdir");
        let outcome = write_pack(temp.path(), &inputs).expect("pack");
        let bytes = std::fs::read(&outcome.pack_path).expect("read pack");

        group.bench_with_input(
            BenchmarkId::new("write", format!("{count}x{size}")),
            &inputs,
            |bencher, inputs| {
                let directory = tempfile::tempdir().expect("tempdir");
                bencher.iter(|| {
                    black_box(write_pack(black_box(directory.path()), black_box(inputs)))
                        .expect("write");
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("ingest", format!("{count}x{size}")),
            &bytes,
            |bencher, bytes| {
                bencher.iter(|| black_box(ingest_pack(black_box(bytes))).expect("ingest"));
            },
        );
    }
    group.finish();
}

criterion_group!(benches, bench_pack);
criterion_main!(benches);
