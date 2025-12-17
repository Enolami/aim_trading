use crate::tasks::task_manager::{register_task, TaskStatus};
use crate::AppWindow;
use aim_data::aim::{fetch_dominance_data, fetch_crypto_rsi_data, fetch_crypto_data, DominanceData, CryptoRsiData, CryptoData};
// Removed fetch_crypto_data import because we will mock/implement the loop locally or assume a new function
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel, Image, SharedPixelBuffer, Rgb8Pixel};
use crate::slint_generatedAppWindow::CryptoData as SlintCryptoData;
use crate::slint_generatedAppWindow::DominanceChartData as SlintDominanceChartData;
use crate::slint_generatedAppWindow::CoinData as SlintCoinData;
use rand::Rng;

// Define the struct matching your updated JSON structure
// #[derive(Debug, Clone)]
// pub struct CryptoData {
//     pub symbol: String,
//     pub interval: String,
//     pub open_time: i64,
//     pub open: Option<f64>,
//     pub high: Option<f64>,
//     pub low: Option<f64>,
//     pub close: Option<f64>,
//     pub volume: Option<f64>,
//     pub close_time: i64,
//     pub quote_asset_volume: Option<f64>,
//     pub trades: i64,
//     pub taker_buy_base_asset_volume: Option<f64>,
//     pub taker_buy_quote_asset_volume: Option<f64>,
// }

// --- CHART DRAWING HELPER (Sparkline) ---
// Generates a simple line chart for the crypto tiles
fn generate_sparkline_buffer(prices: &[f64], is_positive: bool, width: u32, height: u32) -> SharedPixelBuffer<Rgb8Pixel> {
    let mut buffer = SharedPixelBuffer::<Rgb8Pixel>::new(width, height);
    let pixels = buffer.make_mut_slice();

    // 1. Determine Colors based on Trend
    // Matches Palette.zone-oversold (#182B28 -> 24, 43, 40) or Palette.zone-overbought (#381E22 -> 56, 30, 34)
    let bg_color = if is_positive {
        Rgb8Pixel { r: 24, g: 43, b: 40 } 
    } else {
        Rgb8Pixel { r: 56, g: 30, b: 34 } 
    };

    // Matches Palette.accent-green (#10B981) or Palette.accent-red (#EF4444)
    let line_color = if is_positive {
        //Rgb8Pixel { r: 16, g: 185, b: 129 }
        Rgb8Pixel { r: 136, g: 136, b:136}
    } else {
        //Rgb8Pixel { r: 239, g: 68, b: 68 }
        Rgb8Pixel { r: 136, g: 136, b:136}
    };

    // Fill background (Since Slint Rgb8 doesn't support transparency efficiently here without Rgba, 
    // we match the TickerTile background color)
    for pixel in pixels.iter_mut() {
        *pixel = bg_color;
    }

    if prices.is_empty() { return buffer; }

    // 2. Normalize Data
    let min_price = prices.iter().cloned().fold(f64::INFINITY, f64::min);
    let max_price = prices.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let range = max_price - min_price;

    // Padding to keep line away from absolute edges
    let padding_y = 10.0;
    let draw_height = height as f64 - (padding_y * 2.0);

    // Map points to coordinates
    let mut points: Vec<(i32, i32)> = Vec::with_capacity(prices.len());

    for (i, &price) in prices.iter().enumerate() {
        let x = (i as f64 / (prices.len() - 1).max(1) as f64 * (width as f64 - 1.0)) as i32;
        
        let normalized = if range == 0.0 { 0.5 } else { (price - min_price) / range };
        // Flip Y (0 is top)
        let y = (height as f64 - padding_y) - (normalized * draw_height);
        
        points.push((x, y as i32));
    }

    // 3. Draw Line (Simple Bresenham or Stepping)
    for i in 0..points.len().saturating_sub(1) {
        let (x0, y0) = points[i];
        let (x1, y1) = points[i+1];

        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        let mut x = x0;
        let mut y = y0;

        loop {
            if x >= 0 && x < width as i32 && y >= 0 && y < height as i32 {
                let idx = (y as u32 * width + x as u32) as usize;
                pixels[idx] = line_color;
                
                // Simple anti-aliasing / thickness (draw pixel below/right)
                if y + 1 < height as i32 {
                     pixels[idx + width as usize] = line_color;
                }
            }
            if x == x1 && y == y1 { break; }
            let e2 = 2 * err;
            if e2 >= dy { err += dy; x += sx; }
            if e2 <= dx { err += dx; y += sy; }
        }
    }

    buffer
}

// --- DOMINANCE CHART HELPER (Existing) ---
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

    let mut btc_pts = Vec::new();
    let mut eth_pts = Vec::new();
    let mut oth_pts = Vec::new();

    for r in data {
        if duration == 0.0 { continue; }
        let x = ((r.timestamp - min_ts) as f64 / duration * (width as f64 - 1.0)) as i32;
        let val = r.dominance.unwrap_or(0.0);
        let y = ((100.0 - val) / 100.0 * (height as f64 - 1.0)) as i32;

        match r.name.as_str() {
            "Bitcoin Dominance" => btc_pts.push((x, y)),
            "Ethereum Dominance" => eth_pts.push((x, y)),
            _ => oth_pts.push((x, y)),
        }
    }

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

fn generate_raw_dominance_api_data() -> Vec<DominanceData> {
    let mut data = Vec::new();
    let mut rng = rand::rng();
    let start_ts = 1722816000;
    
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

    // The list of symbols you want to display on the dashboard
    let target_symbols = vec!["BTC", "ETH", "BNB", "XRP", "SOL", "TRX", "DOGE", "ADA", "BCH", "LINK", "XLM", "ZEC", "LTC", "SUI", "AVAX", "HBAR", "SHIB", "TON"];

    tokio::spawn(async move {
        let mut task_status = TaskStatus::Running;
        loop {
            if let Ok(status) = rx.try_recv() { if task_status != status { task_status = status; } }
            if task_status != TaskStatus::Running { tokio::time::sleep(std::time::Duration::from_millis(100)).await; continue; }

            // Temporary struct to hold data safely across threads (Image is !Send, SharedPixelBuffer is Send)
            struct TempCryptoData {
                symbol: SharedString,
                open: SharedString,
                high: SharedString,
                low: SharedString,
                close: SharedString,
                change_value: SharedString,
                change_percentage: SharedString,
                is_positive: bool,
                chart_buffer: SharedPixelBuffer<Rgb8Pixel>,
            }
            
            let mut temp_list: Vec<TempCryptoData> = Vec::new();

            // 1. Fetch data for each symbol
            for symbol in &target_symbols {
                let history_result = fetch_crypto_data(symbol).await;
                
                if let Ok(history) = history_result {
                    if history.is_empty() {
                         log::error!("Crypto data for {} returned empty list", symbol);
                         continue;
                    }

                     if let Some(latest) = history.last() {
                        // Parse String to f64 for calculations
                        let open_str = &latest.open;
                        let close_str = &latest.close;
                        let high_str = &latest.high;
                        let low_str = &latest.low;

                        let open = open_str.parse::<f64>().unwrap_or(0.0);
                        let close = close_str.parse::<f64>().unwrap_or(0.0);
                        let high = high_str.parse::<f64>().unwrap_or(0.0);
                        let low = low_str.parse::<f64>().unwrap_or(0.0);
                        
                        // Compare with the start of the fetched history
                        let first_open = if let Some(first) = history.first() {
                            first.open.parse::<f64>().unwrap_or(open)
                        } else {
                            open
                        };

                        let change_val = close - first_open;
                        let change_pct = if first_open != 0.0 { (change_val / first_open) * 100.0 } else { 0.0 };
                        let is_positive = change_val >= 0.0;

                        // 2. Process data for Chart
                        // Map the strings to f64
                        let prices: Vec<f64> = history.iter().map(|c| c.close.parse::<f64>().unwrap_or(0.0)).collect();
                        
                        // 3. Generate SharedPixelBuffer (Send)
                        let chart_buffer = generate_sparkline_buffer(&prices, is_positive, 250, 90);

                        temp_list.push(TempCryptoData {
                            symbol: symbol.replace("USDT", "/USDT").into(),
                            open: format!("{:.2}", open).into(),
                            high: format!("{:.2}", high).into(),
                            low: format!("{:.2}", low).into(),
                            close: format!("{:.2}", close).into(),
                            change_value: format!("{:.2}", change_val).into(),
                            change_percentage: format!("{:.2} %", change_pct).into(),
                            is_positive: is_positive,
                            chart_buffer: chart_buffer,
                        });
                    }
                } else {
                    log::warn!("Failed to fetch crypto data for {}", symbol);
                }
            }

            // 4. Update UI
            let _ = ui_handle.upgrade_in_event_loop(move |ui| {
                let crypto_list_data: Vec<SlintCryptoData> = temp_list.into_iter().map(|t| {
                    SlintCryptoData {
                        symbol: t.symbol,
                        open: t.open,
                        high: t.high,
                        low: t.low,
                        close: t.close,
                        change_value: t.change_value,
                        change_percentage: t.change_percentage,
                        is_positive: t.is_positive,
                        chart_data: Image::from_rgb8(t.chart_buffer),
                    }
                }).collect();

                ui.set_crypto_list(ModelRc::new(VecModel::from(crypto_list_data)));
            });
            
            // Refresh interval
            tokio::time::sleep(std::time::Duration::from_secs(60)).await; 
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
            
            final_data.sort_by_key(|r| r.timestamp);

            let mut last_btc: f64 = 0.0;
            let mut last_eth: f64 = 0.0;
            let mut last_others: f64 = 0.0;

            for r in &final_data {
                match r.name.as_str() {
                    "Bitcoin Dominance" => last_btc = r.dominance.unwrap_or(0.0),
                    "Ethereum Dominance" => last_eth = r.dominance.unwrap_or(0.0),
                    _ => last_others = r.dominance.unwrap_or(0.0),
                }
            }

            let chart_buffer = generate_chart_buffer(&final_data, 800, 400);

            let btc_val = format!("{:.2}%", last_btc);
            let eth_val = format!("{:.2}%", last_eth);
            let others_val = format!("{:.2}%", last_others);
            
            let btc_y = (100.0 - last_btc) as f32;
            let eth_y = (100.0 - last_eth) as f32;
            let others_y = (100.0 - last_others) as f32;

            let _ = ui_handle.upgrade_in_event_loop(move |ui| {
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
                Err(_) => generate_dummy_rsi_data() 
            };

            let crypto_list_data: Vec<SlintCoinData> = final_data.iter().map(|d| {
                let rsi = d.current_rsi.unwrap_or(50.0) as f32;
                let y_pos = 1.0 - (rsi / 100.0);
                let market_cap = d.market_cap.unwrap_or(0.0);

                let safe_market_cap = if market_cap < 1.0 { 1.0 } else {market_cap};
                let cap_log = safe_market_cap.log10() as f32;
                
                let min_log = 7.0;
                let max_log = 13.0;
                
                let normalized_cap = (cap_log - min_log) / (max_log - min_log);
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