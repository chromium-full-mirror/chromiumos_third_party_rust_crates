use anyhow::Context;
use criterion::{criterion_group, criterion_main, Criterion};
use std::io::prelude::*;
use std::process::{Command, Stdio};

fn runner_overhead(c: &mut Criterion) {
    c.bench_function("overhead 100000 passes", |b| {
        let mut caselist_file = tempfile::NamedTempFile::new().unwrap();
        for i in 0..100000 {
            writeln!(caselist_file, "dEQP-GLES2.test.p.{}", i).unwrap();
        }
        let caselist = caselist_file.into_temp_path();

        let deqp_runner = env!("CARGO_BIN_EXE_deqp-runner");

        b.iter(|| {
            let output_dir = tempfile::tempdir().context("Creating output dir").unwrap();

            let mut cmd = Command::new(deqp_runner);
            let child = cmd.stdout(Stdio::null()).stderr(Stdio::null());
            let child = child.arg("run");

            let child = child.arg("--deqp");
            let child = child.arg(deqp_runner);

            let child = child.arg("--output");
            let child = child.arg(output_dir.path());

            let child = child.arg("--caselist");
            let child = child.arg(&caselist);

            child.arg("--");
            child.arg("mock-deqp"); // Passed as the first arg of the "deqp" binary (the deqp-runner we passed as --deqp!) to trigger its mock-deqp mode

            let status = child.status().unwrap();
            assert_eq!(status.code().unwrap(), 0);
        });
    });
}

criterion_group!(benches, runner_overhead);
criterion_main!(benches);
