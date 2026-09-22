use criterion::{black_box, criterion_group, criterion_main, Criterion};
use df_test_protocol::{JobRequest, RepositorySpec, TestAction};

fn benchmark_capability_derivation(c: &mut Criterion) {
    let actions: Vec<TestAction> = (0..1_024)
        .map(|index| match index % 5 {
            0 => TestAction::Checkout,
            1 => TestAction::CargoBuild { release: false },
            2 => TestAction::CargoTest { all_features: true },
            3 => TestAction::CargoClippy {
                deny_warnings: true,
            },
            _ => TestAction::CargoFmtCheck,
        })
        .collect();

    let job = JobRequest::new(
        RepositorySpec {
            url: "https://github.com/example/project.git".into(),
            revision: "main".into(),
        },
        actions,
    );

    c.bench_function("required_capabilities_1024_actions", |b| {
        b.iter(|| black_box(&job).required_capabilities())
    });
}

criterion_group!(benches, benchmark_capability_derivation);
criterion_main!(benches);
