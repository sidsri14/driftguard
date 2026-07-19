use std::{fs, hint::black_box};

use criterion::{criterion_group, criterion_main, Criterion};
use driftguard_cli::{config, env_scan};
use tempfile::tempdir;

fn benchmark_env_scan(criterion: &mut Criterion) {
    let workspace = tempdir().expect("benchmark workspace should be created");
    let source_dir = workspace.path().join("src");
    fs::create_dir_all(&source_dir).expect("source directory should be created");

    for index in 0..500 {
        let contents = format!(
            "export const value{index} = process.env.SERVICE_TOKEN_{key};\n",
            key = index % 20
        );
        fs::write(source_dir.join(format!("module_{index}.ts")), contents)
            .expect("benchmark source should be written");
    }

    let config = config::Config {
        env_files: vec![".env.example".to_string()],
        ignore_dirs: config::default_ignore_dirs(),
        source_globs: vec!["**/*.ts".to_string()],
        ignore_env_keys: Vec::new(),
        prompts: Default::default(),
    };

    criterion.bench_function("scan_500_typescript_files", |benchmark| {
        benchmark.iter(|| black_box(env_scan::scan_env_uses(workspace.path(), &config, None)));
    });
}

criterion_group!(benches, benchmark_env_scan);
criterion_main!(benches);
