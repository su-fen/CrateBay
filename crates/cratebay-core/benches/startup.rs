//! Micro-benchmarks for cratebay-core hot paths.
//!
//! Run with:
//!   cargo bench -p cratebay-core
//!
//! Results are written to `target/criterion/` with HTML reports.

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use cratebay_core::hypervisor::{VmInfo, VmState};
use cratebay_core::store::VmStore;

// ─── create_hypervisor ───────────────────────────────────────────

fn bench_create_hypervisor(c: &mut Criterion) {
    c.bench_function("create_hypervisor", |b| {
        b.iter(|| {
            let _hv = black_box(cratebay_core::create_hypervisor());
        });
    });
}

// ─── platform_info ───────────────────────────────────────────────

fn bench_platform_info(c: &mut Criterion) {
    c.bench_function("platform_info", |b| {
        b.iter(|| {
            let _info = black_box(cratebay_core::platform_info());
        });
    });
}

// ─── VmStore load/save round-trip ────────────────────────────────

fn make_test_vms(count: usize) -> Vec<VmInfo> {
    (0..count)
        .map(|i| VmInfo {
            id: format!("bench-{}", i),
            name: format!("bench-vm-{}", i),
            state: VmState::Stopped,
            cpus: 4,
            memory_mb: 4096,
            disk_gb: 50,
            rosetta_enabled: false,
            shared_dirs: vec![],
            port_forwards: vec![],
            os_image: Some("alpine-3.19".into()),
        })
        .collect()
}

fn bench_vm_store_round_trip(c: &mut Criterion) {
    let mut group = c.benchmark_group("vm_store");

    for count in [1, 10, 50] {
        let vms = make_test_vms(count);

        group.bench_function(format!("save_{}_vms", count), |b| {
            let tmp = tempfile::tempdir().unwrap();
            let store = VmStore::with_path(tmp.path().join("vms.json"));
            b.iter(|| {
                store.save_vms(black_box(&vms)).unwrap();
            });
        });

        group.bench_function(format!("load_{}_vms", count), |b| {
            let tmp = tempfile::tempdir().unwrap();
            let store = VmStore::with_path(tmp.path().join("vms.json"));
            store.save_vms(&vms).unwrap();
            b.iter(|| {
                let loaded = store.load_vms().unwrap();
                black_box(loaded);
            });
        });

        group.bench_function(format!("round_trip_{}_vms", count), |b| {
            let tmp = tempfile::tempdir().unwrap();
            let store = VmStore::with_path(tmp.path().join("vms.json"));
            b.iter(|| {
                store.save_vms(black_box(&vms)).unwrap();
                let loaded = store.load_vms().unwrap();
                black_box(loaded);
            });
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_create_hypervisor,
    bench_platform_info,
    bench_vm_store_round_trip,
);
criterion_main!(benches);
