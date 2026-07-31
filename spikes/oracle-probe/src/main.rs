// Spike descartável — ver docs/adr/0001-camada-oracle.md
// P2: o runtime asupersync (usado internamente pelo crate `oracledb`) coexiste,
// no mesmo processo, com o runtime tokio que o Tauri usa?

use asupersync::runtime::RuntimeBuilder;

#[tokio::main]
async fn main() {
    // Tarefa tokio rodando concorrentemente para provar que o runtime tokio
    // do processo host (Tauri) segue vivo e respondendo.
    let tokio_task = tokio::spawn(async {
        for i in 0..5 {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            println!("[tokio] tick {i}");
        }
        "tokio ok"
    });

    // O runtime do asupersync roda numa thread OS separada — é a única forma
    // documentada de usá-lo, já que `RuntimeBuilder::block_on` bloqueia a
    // thread chamadora.
    let asupersync_thread = std::thread::spawn(|| {
        let runtime = RuntimeBuilder::new()
            .worker_threads(1)
            .build()
            .expect("asupersync runtime should build");

        runtime.block_on(async {
            for i in 0..5 {
                std::thread::sleep(std::time::Duration::from_millis(20));
                println!("[asupersync] tick {i}");
            }
            "asupersync ok"
        })
    });

    let tokio_result = tokio_task.await.expect("tokio task panicked");
    let asupersync_result = asupersync_thread.join().expect("asupersync thread panicked");

    println!("P2 result: tokio={tokio_result} asupersync={asupersync_result}");
    assert_eq!(tokio_result, "tokio ok");
    assert_eq!(asupersync_result, "asupersync ok");
    println!("P2: PASS — os dois runtimes coexistiram no mesmo processo sem panic/deadlock");
}
