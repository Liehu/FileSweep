use std::sync::Arc;

fn main() {
    // 初始化日志
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // 解析简易命令行参数
    let args: Vec<String> = std::env::args().collect();

    let mut headless = false;
    let mut port: u16 = 3210;
    let mut host = "0.0.0.0".to_string();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--headless" => headless = true,
            "--port" => {
                if i + 1 < args.len() {
                    port = args[i + 1].parse().unwrap_or(3210);
                    i += 1;
                }
            }
            "--host" => {
                if i + 1 < args.len() {
                    host = args[i + 1].clone();
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }

    if headless {
        run_headless(port, host);
    } else {
        filesweep_lib::run();
    }
}

fn run_headless(port: u16, host: String) {
    // 初始化配置（与 lib::run 相同逻辑）
    let config = filesweep_lib::core::config::default_config();
    let config_path = filesweep_lib::core::config::default_config_path();

    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let config = if config_path.exists() {
        filesweep_lib::core::config::load_config(config_path.to_string_lossy().to_string().as_str())
            .unwrap_or(config)
    } else {
        config
    };
    let config = Arc::new(config);

    // 初始化数据库
    if let Some(parent) = std::path::Path::new(&config.db_path).parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let db = filesweep_lib::db::catalog::CatalogDB::open(&config.db_path)
        .expect("无法打开数据库");
    db.seed_default_tags().ok();

    // 启动无头模式
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("创建 tokio runtime 失败");

    rt.block_on(async {
        filesweep_lib::headless::run_headless(
            config,
            db,
            filesweep_lib::headless::HeadlessOptions { port, host },
        )
        .await;
    });
}
