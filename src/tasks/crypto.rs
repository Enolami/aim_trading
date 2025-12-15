use crate::tasks::task_manager::{register_task, TaskStatus};
use crate::AppWindow;
use aim_data::aim::{fetch_crypto_data, fetch_dominance_data, CryptoData, DominanceData};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use crate::slint_generatedAppWindow::CryptoData as SlintCryptoData;
use crate::slint_generatedAppWindow::DominanceChartData as SlintDominanceChartData;
use rand::Rng; // For dummy data generation

// Function to simulate backend API response using DominanceData
fn generate_raw_dominance_api_data() -> Vec<DominanceData> {
    let mut data = Vec::new();
    let mut rng = rand::rng();
    let start_ts = 1722816000;
    
    // Generate 50 points
    for i in 0..50 {
        let ts = start_ts + (i * 86400); // Daily points
        
        let base_btc = 55.0 + rng.random_range(-2.0..2.0) + (i as f64 * 0.1); 
        let base_eth = 15.0 + rng.random_range(-1.0..1.0) - (i as f64 * 0.05);
        let base_others = 100.0 - base_btc - base_eth; 

        data.push(DominanceData { 
            name: "Bitcoin Dominance".to_string(), 
            timestamp: ts, 
            dominance: Some(base_btc),
        });
        data.push(DominanceData { 
            name: "Ethereum Dominance".to_string(), 
            timestamp: ts, 
            dominance: Some(base_eth),
        });
        data.push(DominanceData { 
            name: "Others".to_string(), 
            timestamp: ts, 
            dominance: Some(base_others),
        });
    }
    data
}

// ... existing SVG generator for sparklines ...
fn generate_dummy_chart_svg() -> SharedString {
    let mut rng = rand::rng();
    let steps = 20;
    let mut current_val: f64 = 0.5; 
    let mut path_cmd = String::new();
    let step_x = 100.0 / ((steps - 1) as f32);

    for i in 0..steps {
        current_val += rng.random_range(-0.15..0.15);
        current_val = current_val.max(0.0).min(1.0);
        let x = i as f32 * step_x;
        let y = (1.0 - current_val) * 100.0;
        if i == 0 { path_cmd.push_str(&format!("M {:.1} {:.1} ", x, y)); } 
        else { path_cmd.push_str(&format!("L {:.1} {:.1} ", x, y)); }
    }
    path_cmd.into()
}

fn generate_dummy_data() -> Vec<CryptoData> {
    let symbols = vec![
        "ACB", "BCM", "BID", "BVH", "CTG", "FPT", "GAS", "GVR", "HDB", "HPG", 
        "MBB", "MSN", "MWG", "PLX", "POW", "SAB", "SHB", "SSB", "SSI", "STB", 
        "TCB", "TPB", "VCB", "VHM", "VIB", "VIC", "VJC", "VNM", "VPB", "VRE"
    ];
    let mut rng = rand::rng();
    
    symbols.into_iter().map(|sym| {
        CryptoData {
            symbol: sym.to_string(),
            open: Some(rng.random_range(1.0..1000.0)),
            high: Some(rng.random_range(10.0..100.0)),
            low: Some(rng.random_range(1.0..1000.0)), 
            close: Some(rng.random_range(1.0..1000.0)),
            // datetime: "".to_string(),
            volume: Some(rng.random_range(1000.0..100000.0)),
            interval: "".to_string(),
            timestamp: 0,
            // updated_at: "".to_string(),
        }
    }).collect()
}

pub async fn spawn_crypto_task(ui: &AppWindow) -> crate::tasks::task_manager::TaskHandle {
    let ui_handle = ui.as_weak();
    let (tx, mut rx) = tokio::sync::mpsc::channel(10);
    let task_handle =
        register_task("chart.quantitative.crypto".to_string(), tx, "Quantitative Crypto Data".to_string()).await;

    tokio::spawn(async move {
        let mut task_status = TaskStatus::Running;

        loop {
            if let Ok(status) = rx.try_recv() {
                if task_status != status {
                    log::info!("Crypto Data task status changed to: {:?}", status);
                    task_status = status;
                }
            }

            if task_status != TaskStatus::Running {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                continue;
            }

            let fetched_result = fetch_crypto_data().await;

            let final_data = match fetched_result {
                Ok(data) => {
                    if data.is_empty() {
                        log::warn!("Crypto API returned empty. Using Dummy Data for Debugging.");
                        generate_dummy_data()
                        //vec![]
                    } else {
                        log::info!("Fetched {} Crypto records", data.len());
                        generate_dummy_data()
                    }
                },
                Err(e) => {
                    log::error!("Failed to fetch Crypto data: {}. Using Dummy Data.", e);
                    generate_dummy_data()
                    //vec![]
                }
            };

            let crypto_list_data: Vec<SlintCryptoData> = final_data.iter().map(|d| {
                let close = d.close.unwrap_or(0.0);
                let open = d.open.unwrap_or(0.0);
                let change_val = close - open;
                let change_pct = if open != 0.0 { (change_val / open) * 100.0 } else { 0.0 };
                let is_pos = change_val >= 0.0;

                SlintCryptoData {
                    symbol: d.symbol.clone().into(),
                    open: format!("{:.2}", open).into(),
                    high: d.high.map(|v| format!("{:.2}", v)).unwrap_or_default().into(),
                    low: d.low.map(|v| format!("{:.2}", v)).unwrap_or_default().into(),
                    close: format!("{:.2}", close).into(),
                    change_value: format!("{:.2}", change_val).into(),
                    change_percentage: format!("{:.2} %", change_pct).into(),
                    is_positive: is_pos,
                    chart_data: generate_dummy_chart_svg(),
                }
            }).collect();

            let _ = ui_handle.upgrade_in_event_loop(move |ui| {
                ui.set_crypto_list(ModelRc::new(VecModel::from(crypto_list_data)));
                log::info!("Updated Crypto Data in UI");
            });

            tokio::time::sleep(std::time::Duration::from_secs(300)).await; 
        }
    });

    task_handle
}

pub async fn spawn_dominance_task(ui: &AppWindow) -> crate::tasks::task_manager::TaskHandle {
    let ui_handle = ui.as_weak();
    let (tx, mut rx) = tokio::sync::mpsc::channel(10);
    let task_handle =
        register_task("chart.quantitative.dominance".to_string(), tx, "Quantitative Dominance Data".to_string()).await;

    tokio::spawn(async move {
        let mut task_status = TaskStatus::Running;

        loop {
            if let Ok(status) = rx.try_recv() {
                if task_status != status {
                    log::info!("Dominance Data task status changed to: {:?}", status);
                    task_status = status;
                }
            }

            if task_status != TaskStatus::Running {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                continue;
            }

            let fetched_result = fetch_dominance_data().await;

            let final_data = match fetched_result {
                Ok(data) => {
                    if data.is_empty() {
                        log::warn!("Dominance API returned empty. Use Dummy Data for Debugging.");
                        vec![]
                    } else {
                        log::info!("Fetched {} Dominance records", data.len());
                        data
                    }
                },
                Err(e) => {
                    log::error!("Failed to fetch Dominance data: {}. Use Dummy Data.", e);
                    generate_raw_dominance_api_data()
                }
            };

            // INLINE PROCESSING: Convert DominanceData to Slint Chart Data
            let mut btc_points: Vec<(f64, f64)> = Vec::new();
            let mut eth_points: Vec<(f64, f64)> = Vec::new();
            let mut other_points: Vec<(f64, f64)> = Vec::new();

            // Get min/max for normalization
            let min_ts = final_data.iter().map(|r| r.timestamp).min().unwrap_or(0);
            let max_ts = final_data.iter().map(|r| r.timestamp).max().unwrap_or(1);
            let duration = (max_ts - min_ts) as f64;

            for r in &final_data {
                // Normalize X (Time) to 0..100
                let x = if duration == 0.0 { 0.0 } else { ((r.timestamp - min_ts) as f64 / duration) * 100.0 };
                
                // Extract dominance from Option<f64>
                let val = r.dominance.unwrap_or(0.0);
                
                // Normalize Y (Dominance 0-100%) to SVG Y (100-0)
                let y = 100.0 - val;

                match r.name.as_str() {
                    "Bitcoin Dominance" => btc_points.push((x, y)),
                    "Ethereum Dominance" => eth_points.push((x, y)),
                    _ => other_points.push((x, y)),
                }
            }

            // Helper closure to build path string "M x y L x y..."
            let build_path = |points: &Vec<(f64, f64)>| -> SharedString {
                let mut path = String::new();
                for (i, (x, y)) in points.iter().enumerate() {
                    if i == 0 {
                        path.push_str(&format!("M {:.1} {:.1} ", x, y));
                    } else {
                        path.push_str(&format!("L {:.1} {:.1} ", x, y));
                    }
                }
                path.into()
            };

            // Helper to get last value string
            let get_last_val = |points: &Vec<(f64, f64)>| -> SharedString {
                if let Some((_, y)) = points.last() {
                    format!("{:.2}%", 100.0 - y).into()
                } else {
                    "0.00%".into()
                }
            };

            let dominace_chart_data = SlintDominanceChartData {
                btc_path: build_path(&btc_points),
                eth_path: build_path(&eth_points),
                others_path: build_path(&other_points),
                btc_value: get_last_val(&btc_points),
                eth_value: get_last_val(&eth_points),
                others_value: get_last_val(&other_points),
            };

            let _ = ui_handle.upgrade_in_event_loop(move |ui| {
                ui.set_dominance_data(dominace_chart_data);
                log::info!("Updated Dominance Data in UI");
            });

            tokio::time::sleep(std::time::Duration::from_secs(300)).await; 
        }
    });

    task_handle
}