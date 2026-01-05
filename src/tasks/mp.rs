use std::cell::RefCell;
use std::rc::Rc;

use crate::tasks::task_manager::{register_task, TaskStatus};
use crate::AppWindow;
use aim_data::aim::{fetch_rsi14_data, RsiData, fetch_ma50_data, MaData}; // Import RsiData explicitly
use slint::{ComponentHandle, Model, ModelRc, VecModel};
use crate::slint_generatedAppWindow::{
    RsiData as SlintRsiData, 
    CoinData as SlintCoinData,
    MaData as SlintMaData 
};
use rand::Rng; // For dummy data generation

// Helper function to generate dummy data for debugging
fn generate_dummy_data() -> Vec<RsiData> {
    let symbols = vec![
        "ACB", "BCM", "BID", "BVH", "CTG", "FPT", "GAS", "GVR", "HDB", "HPG", 
        "MBB", "MSN", "MWG", "PLX", "POW", "SAB", "SHB", "SSB", "SSI", "STB", 
        "TCB", "TPB", "VCB", "VHM", "VIB", "VIC", "VJC", "VNM", "VPB", "VRE"
    ];
    let mut rng = rand::rng();
    
    symbols.into_iter().map(|sym| {
        RsiData {
            symbol: sym.to_string(),
            value: Some(rng.random_range(20.0..80.0)),
            price: Some(rng.random_range(10.0..100.0)),
            vhtt: Some(rng.random_range(1.0..1000.0)), // Billions
            timestamp: 0,
            interval: "D".to_string(),
        }
    }).collect()
}

fn generate_dummy_ma_data() -> Vec<MaData> {
    let symbols = vec![
        "ACB", "BCM", "BID", "BVH", "CTG", "FPT", "GAS", "GVR", "HDB", "HPG", 
        "MBB", "MSN", "MWG", "PLX", "POW", "SAB", "SHB", "SSB", "SSI", "STB", 
        "TCB", "TPB", "VCB", "VHM", "VIB", "VIC", "VJC", "VNM", "VPB", "VRE"
    ];
    let mut rng = rand::rng();
    
    symbols.into_iter().map(|sym| {
        let price = rng.random_range(50.0..150.0);
        let middle = price + rng.random_range(-10.0..10.0); // Random deviation
        let width = rng.random_range(5.0..15.0);
        MaData {
            symbol: sym.to_string(),
            interval: "D".to_string(),
            timestamp: 0,
            middle: Some(middle),
            upper: Some(middle + width),
            lower: Some(middle - width),
            price: Some(price),
            vhtt: Some(rng.random_range(1.0..1000.0)),
        }
    }).collect()
}

pub async fn spawn_rsi_task(ui: &AppWindow) -> crate::tasks::task_manager::TaskHandle {
    let ui_handle = ui.as_weak();
    let (tx, mut rx) = tokio::sync::mpsc::channel(10);
    let task_handle =
        register_task("chart.quantitative.mp".to_string(), tx, "Quantitative MP (RSI)".to_string()).await;

    tokio::spawn(async move {
        let mut task_status = TaskStatus::Running;

        loop {
            if let Ok(status) = rx.try_recv() {
                if task_status != status {
                    log::info!("Quantitative MP task status changed to: {:?}", status);
                    task_status = status;
                }
            }

            if task_status != TaskStatus::Running {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                continue;
            }

            // Fetch data from API
            let fetched_result = fetch_rsi14_data().await;
            
            // Logic: If API returns empty (or fails), use dummy data for Debugging
            let final_data = match fetched_result {
                Ok(data) => {
                    if data.is_empty() {
                        log::warn!("RSI14 API returned empty. Using Dummy Data for Debugging.");
                        generate_dummy_data()
                    } else {
                        log::info!("Fetched {} RSI14 records", data.len());
                        data
                    }
                },
                Err(e) => {
                    log::error!("Failed to fetch RSI14 data: {}. Using Dummy Data.", e);
                    generate_dummy_data()
                }
            };

            // --- DATA PROCESSING ---

            // 0. Calculate Statistics (Overbought/Oversold Percentages)
            let mut total_count_stats = 0.0;
            let mut overbought_count = 0.0;
            let mut oversold_count = 0.0;

            for d in &final_data {
                // Skip if value is missing
                if let Some(val) = d.value {
                    total_count_stats += 1.0;
                    if val > 70.0 {
                        overbought_count += 1.0;
                    } else if val < 30.0 {
                        oversold_count += 1.0;
                    }
                }
            }

            let (overbought_pct, oversold_pct) = if total_count_stats > 0.0 {
                ((overbought_count / total_count_stats) * 100.0, (oversold_count / total_count_stats) * 100.0)
            } else {
                (0.0, 0.0)
            };

            // Range: Min 10B -> Max 1,000,000B
            let max_log = 7.0; 
            let min_log = 1.0; 

            // Filter and Map: Ignore symbols with null VHTT
            let processed_rsi: Vec<(SlintRsiData, SlintCoinData)> = final_data.iter().filter_map(|d| {
                // Filter: Check if vhtt is present
                let vhtt = d.vhtt?;
                let rsi_val = d.value?; 
                let price = d.price?;

                // 1. Prepare SlintRsiData (Table)
                let table_item = SlintRsiData {
                    symbol: d.symbol.clone().into(),
                    rsi14: format!("{:.2}", rsi_val).into(),
                    price: format!("{:.2}", price).into(),
                    vhtt: format!("{:.2} B", vhtt).into(),
                };

                // 2. Prepare SlintCoinData (Heatmap)
                // Y-AXIS: RSI (Linear 0-100) -> Slint 1.0-0.0
                let y_pos = 1.0 - (rsi_val as f32 / 100.0);

                // X-AXIS: Market Cap (Logarithmic)
                let safe_vhtt = if vhtt < 1.0 { 1.0 } else { vhtt }; 
                let cap_log = safe_vhtt.log10() as f32;
                let normalized_cap = (cap_log - min_log) / (max_log - min_log);
                let x_pos = 1.0 - normalized_cap;

                let coin_item = SlintCoinData {
                    symbol: d.symbol.clone().into(),
                    rsi: rsi_val as f32,
                    x: x_pos.max(0.02).min(0.98), 
                    y: y_pos.max(0.05).min(0.95), 
                };

                Some((table_item, coin_item))
            }).collect();

            // Unzip
            let (rsi_list_data, coin_list_data): (Vec<SlintRsiData>, Vec<SlintCoinData>) = processed_rsi.into_iter().unzip();

            // Update UI in one go
            let _ = ui_handle.upgrade_in_event_loop(move |ui| {
                ui.set_rsi_list(ModelRc::new(VecModel::from(rsi_list_data)));
                ui.set_coin_list(ModelRc::new(VecModel::from(coin_list_data)));
                ui.set_rsi_overbought_pct(overbought_pct);
                ui.set_rsi_oversold_pct(oversold_pct);
            });

            // Refresh rate
            tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
        }
    });

    task_handle
}

pub async fn spawn_ma50_task(ui: &AppWindow) -> crate::tasks::task_manager::TaskHandle {
    let ui_handle = ui.as_weak();
    let (tx, mut rx) = tokio::sync::mpsc::channel(10);
    let task_handle =
        register_task("chart.quantitative.mp.ma50".to_string(), tx, "Quantitative MP MA50".to_string()).await;

    tokio::spawn(async move {
        let mut task_status = TaskStatus::Running;

        loop {
            if let Ok(status) = rx.try_recv() {
                if task_status != status {
                    log::info!("Quantitative MP MA50 task status changed to: {:?}", status);
                    task_status = status;
                }
            }

            if task_status != TaskStatus::Running {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                continue;
            }

            // Fetch data from API
            let fetched_result = fetch_ma50_data().await;
            
            let final_data = match fetched_result {
                Ok(data) => {
                    if data.is_empty() {
                        log::warn!("Ma50 API returned empty. Using Dummy Data for Debugging.");
                        generate_dummy_ma_data()
                    } else {
                        log::info!("Fetched {} Ma50 records", data.len());
                        data
                    }
                },
                Err(e) => {
                    log::error!("Failed to fetch Ma50 data: {}. Using Dummy Data.", e);
                    generate_dummy_ma_data()
                }
            };

            // --- DATA PROCESSING FOR MA50 ---
            
            // X-AXIS Config
            let max_log = 7.0; 
            let min_log = 1.0;

            let mut above_count = 0.0;
            let mut below_count = 0.0;

            // We filter and map data in one pass to avoid recalculating loops
            // Output tuple: (SlintMaData, SlintCoinData) -> (Table Item, Heatmap Item)
            let processed_items: Vec<(SlintMaData, SlintCoinData)> = final_data.iter().filter_map(|d| {
                // Filter: Ignore symbol if vhtt is null (or None)
                let vhtt = d.vhtt?;
                
                // Other required fields
                let price = d.price?;
                let middle = d.middle?;
                let upper = d.upper?;

                // Formula Logic:
                // sigma = (upper - middle) / 2
                let sigma = (upper - middle) / 2.0;

                // Avoid division by zero
                if sigma.abs() < 0.0001 {
                    return None;
                }

                // ma50_score = (price - middle) / sigma
                let ma50_score = (price - middle) / sigma;

                // Stats calculation (Side effect in filter_map is okay here as it runs sequentially)
                if ma50_score > 0.0 {
                    above_count += 1.0;
                } else {
                    below_count += 1.0;
                }

                // --- 1. Prepare SlintCoinData (Heatmap) ---
                
                // X-AXIS: Market Cap (Logarithmic)
                let safe_vhtt = if vhtt < 1.0 { 1.0 } else { vhtt }; 
                let cap_log = safe_vhtt.log10() as f32;
                let normalized_cap = (cap_log - min_log) / (max_log - min_log);
                let x_pos = 1.0 - normalized_cap;

                // Y-AXIS: Score
                // Map Score to Y-axis. 
                // Center (Score 0) is 0.5.
                let y_pos = 0.5 - (ma50_score as f32 / 6.0);

                // COLOR MAPPING FOR 5 COLORS:
                // We map scores to discrete values to trigger the 5-color logic in Frontend.
                // 10.0 = Very Low (Green)
                // 30.0 = Low (Light Green)
                // 50.0 = Mid (Yellow)
                // 70.0 = High (Orange)
                // 90.0 = Very High (Red)
                let proxy_rsi = if ma50_score < -2.0 {
                    10.0 
                } else if ma50_score < -0.5 {
                    30.0 
                } else if ma50_score < 0.5 {
                    50.0 
                } else if ma50_score < 2.0 {
                    70.0 
                } else {
                    90.0 
                };

                let ma_coin_item = SlintCoinData {
                    symbol: d.symbol.clone().into(),
                    rsi: proxy_rsi,
                    x: x_pos.max(0.02).min(0.98),
                    y: y_pos.max(0.05).min(0.95),
                };

                // --- 2. Prepare SlintMaData (Table) ---
                let ma_table_item = SlintMaData {
                    symbol: d.symbol.clone().into(),
                    ma50: format!("{:.2}", ma50_score).into(),
                    price: format!("{:.2}", price).into(),
                    vhtt: format!("{:.2} B", vhtt).into(),
                };

                Some((ma_table_item, ma_coin_item))
            }).collect();

            // Unzip the processed items into two vectors: one for table (MaData), one for chart (CoinData)
            let (ma_list_data, ma_coin_list_data): (Vec<SlintMaData>, Vec<SlintCoinData>) = processed_items.into_iter().unzip();

            // Calculate Percentages
            let total_count = ma_list_data.len() as f32; // Recalculate based on filtered list
            let (above_pct, below_pct) = if total_count > 0.0 {
                ((above_count / total_count) * 100.0, (below_count / total_count) * 100.0)
            } else {
                (0.0, 0.0)
            };

            // Update UI
            let _ = ui_handle.upgrade_in_event_loop(move |ui| {
                ui.set_ma_list(ModelRc::new(VecModel::from(ma_list_data)));
                ui.set_ma_coin_list(ModelRc::new(VecModel::from(ma_coin_list_data)));
                ui.set_ma_above_pct(above_pct);
                ui.set_ma_below_pct(below_pct);
            });

            // Refresh rate
            tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
        }
    });

    task_handle
}

pub fn register_rsi_sort_handler(ui: &AppWindow) {
    log::warn!("Registering RSI sort handler");
    let ui_weak = ui.as_weak();
    
    let sort_ascending = Rc::new(RefCell::new(false));

    ui.on_sort_rsi14(move || {
        log::warn!("RSI sort handler triggered");
        let ui = ui_weak.unwrap();
        
        let model_rc = ui.get_rsi_list();
        let mut items: Vec<SlintRsiData> = model_rc.iter().collect();
        let mut asc = sort_ascending.borrow_mut();
        *asc = !*asc;
        let is_asc = *asc;

        items.sort_by(|a, b| {
            let val_a = a.rsi14.as_str().parse::<f64>().unwrap_or(0.0);
            let val_b = b.rsi14.as_str().parse::<f64>().unwrap_or(0.0);
            
            if is_asc {
                val_a.partial_cmp(&val_b).unwrap_or(std::cmp::Ordering::Equal)
            } else {
                val_b.partial_cmp(&val_a).unwrap_or(std::cmp::Ordering::Equal)
            }
        });

        ui.set_rsi_list(ModelRc::new(VecModel::from(items)));
        log::warn!("RSI List sorted. Ascending: {}", is_asc);
    });
}

pub fn register_ma50_sort_handler(ui: &AppWindow) {
    log::warn!("Registering MA50 sort handler");
    let ui_weak = ui.as_weak();
    
    let sort_ascending = Rc::new(RefCell::new(false));

    ui.on_sort_ma50(move || {
        log::warn!("MA50 sort handler triggered");
        let ui = ui_weak.unwrap();
        
        let model_rc = ui.get_ma_list(); // Get MaData list
        let mut items: Vec<SlintMaData> = model_rc.iter().collect();
        let mut asc = sort_ascending.borrow_mut();
        *asc = !*asc;
        let is_asc = *asc;

        items.sort_by(|a, b| {
            let val_a = a.ma50.as_str().parse::<f64>().unwrap_or(0.0);
            let val_b = b.ma50.as_str().parse::<f64>().unwrap_or(0.0);
            
            if is_asc {
                val_a.partial_cmp(&val_b).unwrap_or(std::cmp::Ordering::Equal)
            } else {
                val_b.partial_cmp(&val_a).unwrap_or(std::cmp::Ordering::Equal)
            }
        });

        ui.set_ma_list(ModelRc::new(VecModel::from(items)));
        log::warn!("MA50 List sorted. Ascending: {}", is_asc);
    });
}
