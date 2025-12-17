use crate::tasks::task_manager::{register_task, TaskStatus};
use crate::AppWindow;
use aim_data::aim::{fetch_crypto_data, fetch_dominance_data, fetch_crypto_rsi_data, CryptoData, DominanceData, CryptoRsiData};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel, Image, SharedPixelBuffer, Rgb8Pixel};
use crate::slint_generatedAppWindow::CryptoData as SlintCryptoData;
use crate::slint_generatedAppWindow::DominanceChartData as SlintDominanceChartData;
use crate::slint_generatedAppWindow::CoinData as SlintCoinData;
use rand::Rng;

// --- CHART DRAWING HELPER ---
// Returns a SharedPixelBuffer which is Send, unlike Image in some contexts
fn generate_chart_buffer(data: &Vec<DominanceData>, width: u32, height: u32) -> SharedPixelBuffer<Rgb8Pixel> {
    let mut buffer = SharedPixelBuffer::<Rgb8Pixel>::new(width, height);
    let pixels = buffer.make_mut_slice();
    
    // Clear background (Dark theme: #141416 -> 20, 20, 22)
    for pixel in pixels.iter_mut() {
        *pixel = Rgb8Pixel { r: 20, g: 20, b: 22 };
    }

    // Colors
    let col_btc = Rgb8Pixel { r: 247, g: 147, b: 26 }; // Orange
    let col_eth = Rgb8Pixel { r: 22, g: 82, b: 240 };  // Blue
    let col_oth = Rgb8Pixel { r: 136, g: 136, b: 136 }; // Grey

    if data.is_empty() {
        return buffer;
    }

    let min_ts = data.iter().map(|r| r.timestamp).min().unwrap_or(0);
    let max_ts = data.iter().map(|r| r.timestamp).max().unwrap_or(1);
    let duration = (max_ts - min_ts) as f64;

    // Helper to draw a line segment
    let draw_line = |pix: &mut [Rgb8Pixel], x0: i32, y0: i32, x1: i32, y1: i32, col: Rgb8Pixel| {
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        let mut x = x0;
        let mut y = y0;

        loop {
            if x >= 0 && x < width as i32 && y >= 0 && y < height as i32 {
                pix[(y as u32 * width + x as u32) as usize] = col;
            }
            if x == x1 && y == y1 { break; }
            let e2 = 2 * err;
            if e2 >= dy { err += dy; x += sx; }
            if e2 <= dx { err += dx; y += sy; }
        }
    };

    // Separate data streams
    let mut btc_pts = Vec::new();
    let mut eth_pts = Vec::new();
    let mut oth_pts = Vec::new();

    for r in data {
        if duration == 0.0 { continue; }
        let x = ((r.timestamp - min_ts) as f64 / duration * (width as f64 - 1.0)) as i32;
        let val = r.dominance.unwrap_or(0.0);
        // Y flip: 0 is top. 100% -> 0, 0% -> height
        let y = ((100.0 - val) / 100.0 * (height as f64 - 1.0)) as i32;

        match r.name.as_str() {
            "Bitcoin Dominance" => btc_pts.push((x, y)),
            "Ethereum Dominance" => eth_pts.push((x, y)),
            _ => oth_pts.push((x, y)),
        }
    }

    // Draw lines
    let mut draw_series = |pts: &Vec<(i32, i32)>, col: Rgb8Pixel| {
        for i in 0..pts.len().saturating_sub(1) {
            let (x0, y0) = pts[i];
            let (x1, y1) = pts[i+1];
            draw_line(pixels, x0, y0, x1, y1, col);
        }
    };

    draw_series(&oth_pts, col_oth);
    draw_series(&eth_pts, col_eth);
    draw_series(&btc_pts, col_btc);

    buffer
}

// Function to simulate backend API response
fn generate_raw_dominance_api_data() -> Vec<DominanceData> {
    let mut data = Vec::new();
    let mut rng = rand::rng();
    let start_ts = 1722816000;
    
    // Generate 100 points for smoother curve
    for i in 0..100 {
        let ts = start_ts + (i * 43200); 
        
        let base_btc = 55.0 + rng.random_range(-2.0..2.0) + (i as f64 * 0.05); 
        let base_eth = 15.0 + rng.random_range(-1.0..1.0) - (i as f64 * 0.02);
        let base_others = 100.0 - base_btc - base_eth; 

        data.push(DominanceData { name: "Bitcoin Dominance".to_string(), timestamp: ts, dominance: Some(base_btc) });
        data.push(DominanceData { name: "Ethereum Dominance".to_string(), timestamp: ts, dominance: Some(base_eth) });
        data.push(DominanceData { name: "Others".to_string(), timestamp: ts, dominance: Some(base_others) });
    }
    data
}

// ... existing SVG generator for sparklines (left side) ...
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
    let symbols = vec!["ACB", "BCM", "BID", "BVH", "CTG", "FPT", "GAS", "GVR", "HDB", "HPG"];
    let mut rng = rand::rng();
    symbols.into_iter().map(|sym| {
        CryptoData {
            symbol: sym.to_string(),
            open: Some(rng.random_range(1.0..1000.0)),
            high: Some(rng.random_range(10.0..100.0)),
            low: Some(rng.random_range(1.0..1000.0)), 
            close: Some(rng.random_range(1.0..1000.0)),
            interval: "1d".to_string(),
            open_time: 1734480000000,
            close_time: 1734566399999,
            volume: Some(rng.random_range(10.0..1000.0)),
            quote_asset_volume: Some(rng.random_range(10.0..1000.0)),
            taker_buy_base_asset_volume: Some(rng.random_range(10.0..1000.0)),
            taker_buy_quote_asset_volume: Some(rng.random_range(10.0..1000.0)),
            trades: rng.random_range(10..1000),
        }
    }).collect()
}

fn generate_dummy_rsi_data() -> Vec<CryptoRsiData> {
    let symbols = vec![
        ("Dummy", "Bitcoin", "bitcoin"),
        ("ETH", "Ethereum", "ethereum"),
        ("BNB", "Binance Coin", "binance-coin"),
        ("SOL", "Solana", "solana"),
        ("ADA", "Cardano", "cardano"),
    ];

    let mut rng = rand::rng();

    symbols
        .into_iter()
        .enumerate()
        .map(|(i, (symbol, name, slug))| {
            CryptoRsiData {
                id: i.to_string(),
                symbol: symbol.to_string(),
                name: name.to_string(),
                slug: slug.to_string(),

                market_cap: Some(rng.random_range(1_000_000_000.0..1_000_000_000_000.0)),
                price: Some(rng.random_range(1.0..100_000.0)),
                price_24h: Some(rng.random_range(-10.0..10.0)),

                current_rsi: Some(rng.random_range(10.0..90.0)),
                last_rsi: Some(rng.random_range(10.0..90.0)),

                rsi_15m: Some(rng.random_range(10.0..90.0)),
                rsi_1h: Some(rng.random_range(10.0..90.0)),
                rsi_4h: Some(rng.random_range(10.0..90.0)),
                rsi_24h: Some(rng.random_range(10.0..90.0)),
                rsi_7d: Some(rng.random_range(10.0..90.0)),

                updated_at: "".to_string(),
            }
        })
        .collect()
}

pub async fn spawn_crypto_task(ui: &AppWindow) -> crate::tasks::task_manager::TaskHandle {
    let ui_handle = ui.as_weak();
    let (tx, mut rx) = tokio::sync::mpsc::channel(10);
    let task_handle = register_task("chart.quantitative.crypto".to_string(), tx, "Quantitative Crypto Data".to_string()).await;

    tokio::spawn(async move {
        let mut task_status = TaskStatus::Running;
        loop {
            if let Ok(status) = rx.try_recv() { if task_status != status { task_status = status; } }
            if task_status != TaskStatus::Running { tokio::time::sleep(std::time::Duration::from_millis(100)).await; continue; }

            let fetched_result = fetch_crypto_data().await;
            let final_data = match fetched_result {
                Ok(data) => if data.is_empty() { generate_dummy_data() } else { data },
                Err(_) => generate_dummy_data() // Using dummy on error
            };

            let crypto_list_data: Vec<SlintCryptoData> = final_data.iter().map(|d| {
                SlintCryptoData {
                    symbol: d.symbol.clone().into(),
                    open: format!("{:.2}", d.open.unwrap_or(0.0)).into(),
                    high: format!("{:.2}", d.high.unwrap_or(0.0)).into(),
                    low: format!("{:.2}", d.low.unwrap_or(0.0)).into(),
                    close: format!("{:.2}", d.close.unwrap_or(0.0)).into(),
                    change_value: format!("{:.2}", d.close.unwrap_or(0.0) - d.open.unwrap_or(0.0)).into(),
                    change_percentage: format!("{:.2} %", ((d.close.unwrap_or(0.0) - d.open.unwrap_or(0.0))/d.open.unwrap_or(1.0)*100.0)).into(),
                    is_positive: (d.close.unwrap_or(0.0) >= d.open.unwrap_or(0.0)),
                    chart_data: generate_dummy_chart_svg(), 
                }
            }).collect();

            let _ = ui_handle.upgrade_in_event_loop(move |ui| {
                ui.set_crypto_list(ModelRc::new(VecModel::from(crypto_list_data)));
            });
            tokio::time::sleep(std::time::Duration::from_secs(300)).await; 
        }
    });
    task_handle
}

pub async fn spawn_dominance_task(ui: &AppWindow) -> crate::tasks::task_manager::TaskHandle {
    let ui_handle = ui.as_weak();
    let (tx, mut rx) = tokio::sync::mpsc::channel(10);
    let task_handle = register_task("chart.quantitative.dominance".to_string(), tx, "Quantitative Dominance Data".to_string()).await;

    tokio::spawn(async move {
        let mut task_status = TaskStatus::Running;
        loop {
            if let Ok(status) = rx.try_recv() { if task_status != status { task_status = status; } }
            if task_status != TaskStatus::Running { tokio::time::sleep(std::time::Duration::from_millis(100)).await; continue; }

            let fetched_result = fetch_dominance_data().await;
            let mut final_data = match fetched_result {
                Ok(data) => if data.is_empty() { generate_raw_dominance_api_data() } else { data },
                Err(_) => generate_raw_dominance_api_data()
            };
            
            // Sort by time
            final_data.sort_by_key(|r| r.timestamp);

            // Calculate last values for end tags
            let mut last_btc: f64 = 0.0;
            let mut last_eth: f64 = 0.0;
            let mut last_others: f64 = 0.0;

            // Iterate to find the last value for each
            // Note: Since we sorted by timestamp, the last occurrence of each type is the latest
            for r in &final_data {
                match r.name.as_str() {
                    "Bitcoin Dominance" => last_btc = r.dominance.unwrap_or(0.0),
                    "Ethereum Dominance" => last_eth = r.dominance.unwrap_or(0.0),
                    _ => last_others = r.dominance.unwrap_or(0.0),
                }
            }

            // Generate BUFFER (not Image) in the thread
            let chart_buffer = generate_chart_buffer(&final_data, 800, 400);

            // Capture values to move into closure
            let btc_val = format!("{:.2}%", last_btc);
            let eth_val = format!("{:.2}%", last_eth);
            let others_val = format!("{:.2}%", last_others);
            
            // Y position calculation (0 to 100, where 0 is top/100%, 100 is bottom/0%)
            // So if dominance is 60%, y should be 40.
            let btc_y = (100.0 - last_btc) as f32;
            let eth_y = (100.0 - last_eth) as f32;
            let others_y = (100.0 - last_others) as f32;

            let _ = ui_handle.upgrade_in_event_loop(move |ui| {
                // Convert buffer to Image INSIDE the UI thread closure
                let chart_img = Image::from_rgb8(chart_buffer);
                
                let dominace_chart_data = SlintDominanceChartData {
                    chart_image: chart_img,
                    btc_value: btc_val.into(),
                    eth_value: eth_val.into(),
                    others_value: others_val.into(),
                    btc_y: btc_y,
                    eth_y: eth_y,
                    others_y: others_y,
                };
                
                ui.set_dominance_data(dominace_chart_data);
                log::info!("Updated Dominance Data Image & Tags in UI");
            });

            tokio::time::sleep(std::time::Duration::from_secs(300)).await; 
        }
    });
    task_handle
}

pub async fn spawn_crypto_rsi_task(ui: &AppWindow) -> crate::tasks::task_manager::TaskHandle {
    let ui_handle = ui.as_weak();
    let (tx, mut rx) = tokio::sync::mpsc::channel(10);
    let task_handle = register_task("chart.quantitative.crypto_rsi".to_string(), tx, "Quantitative Crypto RSI Data".to_string()).await;

    tokio::spawn(async move {
        let mut task_status = TaskStatus::Running;
        loop {
            if let Ok(status) = rx.try_recv() { if task_status != status { task_status = status; } }
            if task_status != TaskStatus::Running { tokio::time::sleep(std::time::Duration::from_millis(100)).await; continue; }

            let fetched_result = fetch_crypto_rsi_data().await;
            let final_data = match fetched_result {
                Ok(data) => if data.is_empty() { vec![] } else { data },
                Err(_) => generate_dummy_rsi_data() // Using dummy on error
            };

            let crypto_list_data: Vec<SlintCoinData> = final_data.iter().map(|d| {
                let rsi = d.current_rsi.unwrap_or(50.0) as f32;
                let y_pos = 1.0 - (rsi / 100.0);
                let market_cap = d.market_cap.unwrap_or(0.0);

                let safe_market_cap = if market_cap < 1.0 { 1.0 } else {market_cap};
                let cap_log = safe_market_cap.log10() as f32;
                
                // ADJUSTED SCALING for realistic Crypto Market Caps ($10M to $10T)
                // Min Cap ~ $10,000,000 (Log10 = 7.0)
                // Max Cap ~ $10,000,000,000,000 (Log10 = 13.0)
                let min_log = 7.0;
                let max_log = 13.0;
                
                let normalized_cap = (cap_log - min_log) / (max_log - min_log);
                
                // X axis: 1.0 (Right) is small cap (normalized 0), 0.0 (Left) is big cap (normalized 1)
                let x_pos: f32 = 1.0 - normalized_cap;
                
                SlintCoinData {
                    rsi: rsi,
                    symbol: d.symbol.clone().into(),
                    x: x_pos.max(0.02).min(0.98),
                    y: y_pos.max(0.05).min(0.95),
                }
            }).collect();

            let _ = ui_handle.upgrade_in_event_loop(move |ui| {
                ui.set_crypto_coin_list(ModelRc::new(VecModel::from(crypto_list_data)));
            });
            tokio::time::sleep(std::time::Duration::from_secs(300)).await; 
        }
    });
    task_handle
}