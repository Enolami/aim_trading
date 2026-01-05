use crate::tasks::task_manager::{register_task, TaskStatus};
use crate::AppWindow;
use aim_data::aim::{CryptoRsiData, DominanceData, EtfFlowData, CryptoMarketCapData, fetch_crypto_data, fetch_crypto_market_cap_data, fetch_crypto_rsi_data, fetch_dominance_data, fetch_etf_flow_data };
// Removed fetch_crypto_data import because we will mock/implement the loop locally or assume a new function
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel, Image, SharedPixelBuffer, Rgb8Pixel};
use crate::slint_generatedAppWindow::CryptoData as SlintCryptoData;
use crate::slint_generatedAppWindow::DominanceChartData as SlintDominanceChartData;
use crate::slint_generatedAppWindow::CoinData as SlintCoinData;
use crate::slint_generatedAppWindow::EtfFlowData as SlintEtfFlowData;
use crate::slint_generatedAppWindow::CryptoMarketCapData as SlintCryptoMarketCapData;
use rand::Rng;

// --- CHART DRAWING HELPER (Sparkline) ---
// Generates a simple line chart for the crypto tiles
fn generate_sparkline_buffer(prices: &[f64], is_positive: bool, change_pct: f64, width: u32, height: u32) -> SharedPixelBuffer<Rgb8Pixel> {
    let mut buffer = SharedPixelBuffer::<Rgb8Pixel>::new(width, height);
    let pixels = buffer.make_mut_slice();

    // 1. Determine Colors based on Trend
    // Matches Palette.zone-oversold (#182B28 -> 24, 43, 40) or Palette.zone-overbought (#381E22 -> 56, 30, 34)
    let bg_color = if is_positive {
        if change_pct < 10.0 {
            Rgb8Pixel { r: 0, g: 60, b: 0 } 
        } else {
            Rgb8Pixel { r: 0, g: 90, b: 0}
        }
    } else {
        if change_pct > -10.0 {
            Rgb8Pixel { r: 90, g: 0, b: 0 } 
        } else {
            Rgb8Pixel { r: 139, g: 0, b: 0 }
        }
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

fn generate_etf_chart_buffer(data: &[EtfFlowData], width: u32, height: u32) -> SharedPixelBuffer<Rgb8Pixel> {
    let mut buffer = SharedPixelBuffer::<Rgb8Pixel>::new(width, height);
    let pixels = buffer.make_mut_slice();

    // 1. Background (Match Palette.card-bg)
    let bg_color = Rgb8Pixel { r: 26, g: 26, b: 26 };
    for pixel in pixels.iter_mut() { *pixel = bg_color; }

    if data.is_empty() { return buffer; }

    // 2. Define Colors
    // Standard Colors
    let btc_color = Rgb8Pixel { r: 247, g: 147, b: 26 }; // BTC Orange
    let eth_color = Rgb8Pixel { r: 22, g: 82, b: 240 };  // ETH Blue
    
    // Darker Colors for "Over Threshold" area
    let btc_dark = Rgb8Pixel { r: 168, g: 100, b: 18 }; 
    let eth_dark = Rgb8Pixel { r: 11, g: 41, b: 120 };

    // 3. Find Range (Min/Max based on Stacked Totals)
    let mut max_abs_val = 0.0;
    
    for d in data {
        let btc = d.btc_value.unwrap_or(0.0);
        let eth = d.eth_value.unwrap_or(0.0);
        
        // Sum positives and negatives separately to find stack height
        let pos_sum = (if btc > 0.0 { btc } else { 0.0 }) + (if eth > 0.0 { eth } else { 0.0 });
        let neg_sum = (if btc < 0.0 { btc } else { 0.0 }) + (if eth < 0.0 { eth } else { 0.0 });
        
        if pos_sum.abs() > max_abs_val { max_abs_val = pos_sum.abs(); }
        if neg_sum.abs() > max_abs_val { max_abs_val = neg_sum.abs(); }
    }

    let graph_limit = if max_abs_val == 0.0 { 100.0 } else { max_abs_val * 1.1 };
    
    // Define Threshold (e.g., 50% of max graph value triggers dark mode for that specific crypto)
    // We use 0.5 here so it triggers more often for individual components like ETH which might be smaller than the total stack.
    let threshold_val = graph_limit * 0.5;

    let zero_y = height as f64 / 2.0;

    // 4. Draw Zero Line
    let axis_color = Rgb8Pixel { r: 60, g: 60, b: 60 };
    let zy_int = zero_y as i32;
    if zy_int >= 0 && zy_int < height as i32 {
        for x in 0..width { 
            pixels[(zy_int as u32 * width + x) as usize] = axis_color;
        }
    }

    // 5. Draw Stacked Bars
    let bar_width = (width as f64 / data.len() as f64) * 0.6; 
    let gap = (width as f64 / data.len() as f64) * 0.4;

    for (i, d) in data.iter().enumerate() {
        let btc = d.btc_value.unwrap_or(0.0);
        let eth = d.eth_value.unwrap_or(0.0);
        
        if btc == 0.0 && eth == 0.0 { continue; }

        // Determine if the *entire* block for this crypto should be dark
        let is_btc_dark = btc.abs() > threshold_val;
        let is_eth_dark = eth.abs() > threshold_val;

        let x_start = (i as f64 * (width as f64 / data.len() as f64)) + (gap / 2.0);
        let x_end = x_start + bar_width;

        // Convert value to pixel height
        let to_px = |v: f64| -> f64 { (v.abs() / graph_limit) * (height as f64 / 2.0) };

        // We draw per column (bx) to easily handle pixel-perfect stacking
        let slot_width = width as f64 / data.len() as f64;
        let bar_px = (slot_width * 0.6).round() as i32;
        let gap_px = (slot_width * 0.4).round() as i32;

        let bx_start = i as i32 * (bar_px + gap_px) + gap_px / 2;
        let bx_end = bx_start + bar_px;

        for bx in bx_start..bx_end {
            if bx < 0 || bx >= width as i32 { continue; }

            // --- Positive Flow Stack ---
            let btc_h = if btc > 0.0 { to_px(btc) } else { 0.0 };
            let eth_h = if eth > 0.0 { to_px(eth) } else { 0.0 };
            
            if btc_h > 0.0 || eth_h > 0.0 {
                // Stack: BTC bottom, ETH top
                let y_btc_end = zero_y - btc_h;
                let y_eth_end = y_btc_end - eth_h;

                for by in (y_eth_end as i32)..(zero_y as i32) {
                    if by < 0 || by >= height as i32 { continue; }

                    let color = if (by as f64) > y_btc_end {
                        // In BTC region
                        if is_btc_dark { btc_dark } else { btc_color }
                    } else {
                        // In ETH region
                        if is_eth_dark { eth_dark } else { eth_color }
                    };
                    
                    pixels[(by as u32 * width + bx as u32) as usize] = color;
                }
            }

            // --- Negative Flow Stack ---
            let btc_h_neg = if btc < 0.0 { to_px(btc) } else { 0.0 };
            let eth_h_neg = if eth < 0.0 { to_px(eth) } else { 0.0 };

            if btc_h_neg > 0.0 || eth_h_neg > 0.0 {
                // Stack: BTC top (near 0), ETH bottom
                let y_btc_end = zero_y + btc_h_neg;
                let y_eth_end = y_btc_end + eth_h_neg;

                for by in (zero_y as i32)..(y_eth_end as i32) {
                    if by < 0 || by >= height as i32 { continue; }

                    let color = if (by as f64) < y_btc_end {
                        // In BTC region
                        if is_btc_dark { btc_dark } else { btc_color }
                    } else {
                        // In ETH region
                        if is_eth_dark { eth_dark } else { eth_color }
                    };

                    pixels[(by as u32 * width + bx as u32) as usize] = color;
                }
            }
        }
    }
    buffer
}

fn cubic_interpolate(y0: f64, y1: f64, y2: f64, y3: f64, t: f64) -> f64 {
    let a = -0.5 * y0 + 1.5 * y1 - 1.5 * y2 + 0.5 * y3;
    let b = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
    let c = -0.5 * y0 + 0.5 * y2;
    let d = y1;
    a * t * t * t + b * t * t + c * t + d
}

fn generate_market_cap_chart_buffer(data: &[CryptoMarketCapData], width: u32, height: u32) -> SharedPixelBuffer<Rgb8Pixel> {
    let mut buffer = SharedPixelBuffer::<Rgb8Pixel>::new(width, height);
    let pixels = buffer.make_mut_slice();

    // Background color #141416
    let bg_color = Rgb8Pixel { r: 20, g: 20, b: 22 };
    for pixel in pixels.iter_mut() { *pixel = bg_color; }

    if data.is_empty() { return buffer; }

    // Colors
    let col_others = Rgb8Pixel { r: 136, g: 136, b: 136 }; // Others (Grey)
    let col_stable = Rgb8Pixel { r: 30, g: 132, b: 73 };   // Stable (Dark Green)
    let col_eth = Rgb8Pixel { r: 22, g: 82, b: 240 };      // ETH (Blue)
    let col_btc = Rgb8Pixel { r: 247, g: 147, b: 26 };     // BTC (Orange)

    // Calculate Max Total Market Cap for Y-Axis Scaling
    let mut max_cap = 0.0;
    for d in data {
        let total = d.market_cap.unwrap_or(
            d.btc_value.unwrap_or(0.0) + 
            d.eth_value.unwrap_or(0.0) + 
            d.stable_value.unwrap_or(0.0) + 
            d.other_value.unwrap_or(0.0)
        );
        if total > max_cap { max_cap = total; }
    }

    if max_cap == 0.0 { max_cap = 1.0; } 

    let draw_height = height as f64;
    let data_len = data.len() as isize;

    // Drawing Logic: Interpolate values using Catmull-Rom Spline
    for x in 0..width {
        // 1. Calculate precise position in data array
        let t_ratio = x as f64 / (width as f64);
        let pos = t_ratio * (data_len - 1) as f64;
        
        let idx1 = pos.floor() as isize;     // Current point
        let idx0 = idx1 - 1;                 // Previous point
        let idx2 = idx1 + 1;                 // Next point
        let idx3 = idx1 + 2;                 // Next next point
        
        let t = pos - idx1 as f64; // Fractional part for interpolation

        // Safe index access helper
        let get_data = |idx: isize| -> &CryptoMarketCapData {
            let i = idx.max(0).min(data_len - 1) as usize;
            &data[i]
        };

        let p0 = get_data(idx0);
        let p1 = get_data(idx1);
        let p2 = get_data(idx2);
        let p3 = get_data(idx3);

        // Interpolate each component
        let interp = |extractor: fn(&CryptoMarketCapData) -> f64| -> f64 {
            let v0 = extractor(p0);
            let v1 = extractor(p1);
            let v2 = extractor(p2);
            let v3 = extractor(p3);
            cubic_interpolate(v0, v1, v2, v3, t).max(0.0) // Clamp to 0 to avoid negative artifacts
        };

        // Extractors
        let get_others = |d: &CryptoMarketCapData| d.other_value.unwrap_or(0.0);
        let get_stable = |d: &CryptoMarketCapData| d.stable_value.unwrap_or(0.0);
        let get_eth = |d: &CryptoMarketCapData| d.eth_value.unwrap_or(0.0);
        let get_btc = |d: &CryptoMarketCapData| d.btc_value.unwrap_or(0.0);

        let v_others = interp(get_others);
        let v_stable = interp(get_stable);
        let v_eth = interp(get_eth);
        let v_btc = interp(get_btc);

        // Stack Calculations (Cumulative)
        let h1 = v_others;
        let h2 = h1 + v_stable;
        let h3 = h2 + v_eth;
        let h4 = h3 + v_btc; 

        // Convert to Y coordinates (0 is Top)
        let y_base = height as i32; 
        let y1 = (draw_height - (h1 / max_cap * draw_height)) as i32;
        let y2 = (draw_height - (h2 / max_cap * draw_height)) as i32;
        let y3 = (draw_height - (h3 / max_cap * draw_height)) as i32;
        let y4 = (draw_height - (h4 / max_cap * draw_height)) as i32;

        // Draw Vertical Strips for this X
        // Others (Bottom Layer)
        for y in y1..y_base {
            if y >= 0 && y < height as i32 { pixels[(y as u32 * width + x) as usize] = col_others; }
        }
        // Stable
        for y in y2..y1 {
            if y >= 0 && y < height as i32 { pixels[(y as u32 * width + x) as usize] = col_stable; }
        }
        // ETH
        for y in y3..y2 {
            if y >= 0 && y < height as i32 { pixels[(y as u32 * width + x) as usize] = col_eth; }
        }
        // BTC (Top Layer)
        for y in y4..y3 {
            if y >= 0 && y < height as i32 { pixels[(y as u32 * width + x) as usize] = col_btc; }
        }
    }

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

fn generate_dummy_etf_data() -> Vec<EtfFlowData> {
    let mut data = Vec::new();
    let mut rng = rand::rng();
    let start_ts = 1722816000;
    for i in 0..30 {
        let ts = start_ts + (i * 86400);
        let val = rng.random_range(-500_000_000.0..500_000_000.0);
        data.push(EtfFlowData { timestamp: ts, value: Some(val), btc_value: Some(val * 0.8), eth_value: Some(val * 0.2) });
    }
    data
}

fn generate_dummy_market_cap_data() -> Vec<CryptoMarketCapData> {
    let mut data = Vec::new();
    let mut rng = rand::rng();
    let start_ts = 1722816000; // Aug 2024
    let mut btc = 1_200_000_000_000.0;
    let mut eth = 400_000_000_000.0;
    let mut stable = 160_000_000_000.0;
    let mut others = 600_000_000_000.0;

    for i in 0..100 {
        let ts = start_ts + (i * 86400); // Daily
        // Random Walk
        btc *= rng.random_range(0.98..1.02);
        eth *= rng.random_range(0.97..1.03);
        stable *= rng.random_range(0.999..1.001); // Stablecoins move less
        others *= rng.random_range(0.96..1.04); // Others volatile

        let total = btc + eth + stable + others;

        data.push(CryptoMarketCapData {
            timestamp: ts,
            market_cap: Some(total),
            volume: Some(total * 0.05),
            btc_value: Some(btc),
            eth_value: Some(eth),
            stable_value: Some(stable),
            other_value: Some(others),
        });
    }
    data
}

fn format_currency_short(val: f64) -> String {
    if val >= 1_000_000_000_000.0 {
        format!("${:.2}T", val / 1_000_000_000_000.0)
    } else if val >= 1_000_000_000.0 {
        format!("${:.2}B", val / 1_000_000_000.0)
    } else {
        format!("${:.2}M", val / 1_000_000.0)
    }
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
                        let chart_buffer = generate_sparkline_buffer(&prices, is_positive, change_pct, 250, 90);

                        temp_list.push(TempCryptoData {
                            symbol: symbol.replace("USDT", "/USDT").into(),
                            open: format!("{:.2}", open).into(),
                            high: format!("{:.2}", high).into(),
                            low: format!("{:.2}", low).into(),
                            close: format!("{:.2}", close).into(),
                            change_value: format!("{:.2}", change_val).into(),
                            change_percentage: format!("{:.2}", change_pct).into(),
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
                Err(_) => { generate_dummy_rsi_data()} 
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

pub async fn spawn_etf_flow_task(ui: &AppWindow) -> crate::tasks::task_manager::TaskHandle {
    let ui_handle = ui.as_weak();
    let (tx, mut rx) = tokio::sync::mpsc::channel(10);
    let task_handle = register_task("chart.quantitative.etf_flow".to_string(), tx, "Quantitative EtfFlow Data".to_string()).await;

    tokio::spawn(async move {
        let mut task_status = TaskStatus::Running;
        loop {
            if let Ok(status) = rx.try_recv() { if task_status != status { task_status = status; } }
            if task_status != TaskStatus::Running { tokio::time::sleep(std::time::Duration::from_millis(100)).await; continue; }

            let fetched_result = fetch_etf_flow_data().await;
            let final_data = match fetched_result { 
                Ok(data) => if data.is_empty() { generate_dummy_etf_data() } else { data }, 
                Err(_) => generate_dummy_etf_data() 
            };

            // Calculate Totals properly for the header
            let total_flow: f64 = final_data.iter().map(|d| {
                 let b = d.btc_value.unwrap_or(0.0);
                 let e = d.eth_value.unwrap_or(0.0);
                 d.value.unwrap_or(b + e)
            }).sum();

            let is_positive = total_flow >= 0.0;
            let sign = if is_positive { "+" } else { "-" };
            let abs_flow = total_flow.abs();
            let formatted_flow = if abs_flow >= 1_000_000_000.0 { 
                format!("{}${:.2}B", sign, abs_flow / 1_000_000_000.0) 
            } else if abs_flow >= 1_000_000.0 { 
                format!("{}${:.2}M", sign, abs_flow / 1_000_000.0) 
            } else { 
                format!("{}${:.2}", sign, abs_flow) 
            };

            // Calculate Scale for Y-Axis Labels based on STACKED components
            let mut max_abs_val = 0.0;
            for d in &final_data {
                let btc = d.btc_value.unwrap_or(0.0);
                let eth = d.eth_value.unwrap_or(0.0);
                let pos = (if btc > 0.0 { btc } else { 0.0 }) + (if eth > 0.0 { eth } else { 0.0 });
                let neg = (if btc < 0.0 { btc } else { 0.0 }) + (if eth < 0.0 { eth } else { 0.0 });
                let m = pos.abs().max(neg.abs());
                if m > max_abs_val { max_abs_val = m; }
            }
            
            let graph_limit = if max_abs_val == 0.0 { 100.0 } else { max_abs_val * 1.1 };
            
            let y_max_str = format!("{:.1}M", graph_limit / 1_000_000.0);
            let y_min_str = format!("-{:.1}M", graph_limit / 1_000_000.0);

            // Generate X-Axis Labels
            let month_names = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
            let mut x_labels_vec: Vec<SharedString> = Vec::new();
            
            let step = if final_data.len() > 1 { (final_data.len() - 1) / 4 } else { 1 };
            for i in (0..final_data.len()).step_by(step.max(1)) {
                let ts = final_data[i].timestamp;
                let month_idx = ((ts / 2629743) % 12) as usize; 
                let m_name = month_names[month_idx];
                x_labels_vec.push(m_name.to_string().into());
                if x_labels_vec.len() >= 5 { break; }
            }
            
            let chart_buffer = generate_etf_chart_buffer(&final_data, 800, 400);

            let _ = ui_handle.upgrade_in_event_loop(move |ui| {
                let x_labels_rc = ModelRc::new(VecModel::from(x_labels_vec));
                let chart_img = Image::from_rgb8(chart_buffer);
                let etf_data = SlintEtfFlowData {
                    header_value: formatted_flow.into(),
                    header_subtext: "Total Net Flow".into(),
                    is_positive: is_positive,
                    chart_image: chart_img,
                    y_max: y_max_str.into(),
                    y_min: y_min_str.into(),
                    x_labels: x_labels_rc,
                };
                ui.set_etf_flow_data(etf_data);
            });
            tokio::time::sleep(std::time::Duration::from_secs(300)).await; 
        }
    });
    task_handle
}
pub async fn spawn_crypto_market_cap_task(ui: &AppWindow) -> crate::tasks::task_manager::TaskHandle {
    let ui_handle = ui.as_weak();
    let (tx, mut rx) = tokio::sync::mpsc::channel(10);
    let task_handle = register_task("chart.quantitative.crypto_market_cap".to_string(), tx, "Quantitative Crypto Market Cap Data".to_string()).await;

    tokio::spawn(async move {
        let mut task_status = TaskStatus::Running;
        loop {
            if let Ok(status) = rx.try_recv() { if task_status != status { task_status = status; } }
            if task_status != TaskStatus::Running { tokio::time::sleep(std::time::Duration::from_millis(100)).await; continue; }

            // 1. Fetch Data (mock if empty)
            let fetched_result = fetch_crypto_market_cap_data().await;
            
            let mut final_data = match fetched_result {
                Ok(data) => if data.is_empty() { generate_dummy_market_cap_data() } else { data },
                Err(_) => generate_dummy_market_cap_data()
            };

            final_data.sort_by_key(|d| d.timestamp);

            // 2. Process Statistics (Last vs Prev)
            let last = final_data.last().cloned().unwrap_or(CryptoMarketCapData { timestamp:0, market_cap:None, volume:None, btc_value:None, eth_value:None, stable_value:None, other_value:None });
            // Get data from ~24h ago (or previous point)
            let prev_idx = if final_data.len() > 1 { final_data.len() - 2 } else { 0 };
            let prev = final_data.get(prev_idx).cloned().unwrap_or(last.clone());

            let fmt_change = |curr: f64, old: f64| -> String {
                let diff = curr - old;
                let pct = if old != 0.0 { (diff / old) * 100.0 } else { 0.0 };
                let sign = if pct >= 0.0 { "+" } else { "" };
                format!("{}{:.1}%", sign, pct)
            };

            let btc_v = last.btc_value.unwrap_or(0.0);
            let eth_v = last.eth_value.unwrap_or(0.0);
            let stable_v = last.stable_value.unwrap_or(0.0);
            let other_v = last.other_value.unwrap_or(0.0);
            let total_v = last.market_cap.unwrap_or(btc_v + eth_v + stable_v + other_v);

            // 3. Generate Axis Labels based on Data
            
            // Y-Axis Labels (5 ticks: Max -> 0)
            let mut max_cap_val = 0.0;
            for d in &final_data {
                let total = d.market_cap.unwrap_or(
                    d.btc_value.unwrap_or(0.0) + d.eth_value.unwrap_or(0.0) + d.stable_value.unwrap_or(0.0) + d.other_value.unwrap_or(0.0)
                );
                if total > max_cap_val { max_cap_val = total; }
            }
            if max_cap_val == 0.0 { max_cap_val = 1.0; }

            let mut y_labels_vec: Vec<SharedString> = Vec::new();
            for i in 0..5 {
                // 100%, 75%, 50%, 25%, 0%
                let val = max_cap_val * (1.0 - (i as f64 * 0.25));
                y_labels_vec.push(format_currency_short(val).into());
            }

            // X-Axis Labels (7 ticks)
            let month_names = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
            let mut x_labels_vec: Vec<SharedString> = Vec::new();
            if !final_data.is_empty() {
                let count = 7;
                let step = if final_data.len() > 1 { (final_data.len() - 1) as f64 / (count - 1) as f64 } else { 0.0 };
                
                for i in 0..count {
                    let idx = (i as f64 * step).round() as usize;
                    if idx < final_data.len() {
                        let ts = final_data[idx].timestamp;
                        let seconds_per_day = 86400;
                        let days_since_epoch = ts / seconds_per_day;
                        let year = 1970 + (days_since_epoch / 365);
                        let day_of_year = days_since_epoch % 365;
                        let month_idx = ((day_of_year / 30) % 12) as usize; 
                        let day_of_month = (day_of_year % 30) + 1;
                        
                        let m_idx = ((ts / 2629743) % 12) as usize;
                        let m_name = month_names[m_idx];
                        let label = format!("{} {}", day_of_month, m_name);
                        x_labels_vec.push(label.into());
                    }
                }
            }

            // 4. Generate Chart Image
            let chart_buffer = generate_market_cap_chart_buffer(&final_data, 800, 400);

            // 5. Update UI
            let _ = ui_handle.upgrade_in_event_loop(move |ui| {
                let chart_img = Image::from_rgb8(chart_buffer);
                
                let market_data = SlintCryptoMarketCapData {
                    chart_image: chart_img,
                    total_cap: format_currency_short(total_v).into(),
                    btc_value: format_currency_short(btc_v).into(),
                    btc_change: fmt_change(btc_v, prev.btc_value.unwrap_or(btc_v)).into(),
                    eth_value: format_currency_short(eth_v).into(),
                    eth_change: fmt_change(eth_v, prev.eth_value.unwrap_or(eth_v)).into(),
                    stable_value: format_currency_short(stable_v).into(),
                    stable_change: fmt_change(stable_v, prev.stable_value.unwrap_or(stable_v)).into(),
                    others_value: format_currency_short(other_v).into(),
                    others_change: fmt_change(other_v, prev.other_value.unwrap_or(other_v)).into(),
                    // New dynamic labels
                    y_labels: ModelRc::new(VecModel::from(y_labels_vec)),
                    x_labels: ModelRc::new(VecModel::from(x_labels_vec)),
                };
                
                ui.set_market_cap_data(market_data);
            });

            tokio::time::sleep(std::time::Duration::from_secs(300)).await; 
        }
    });
    task_handle
}
