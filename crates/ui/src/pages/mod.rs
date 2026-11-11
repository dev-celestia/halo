pub mod agent;
pub mod api_mock;
pub mod api_override;
pub mod chrome;
pub mod coder;
pub mod colors;
pub mod comparer;
pub mod crawler;
pub mod diagram;
pub mod doctor;
pub mod editors;
pub mod file_explorer;
pub mod http;
pub mod intercept;
pub mod intruder;
pub mod jwt;
pub mod notes;
pub mod port_scanner;
pub mod r2_storage;
pub mod repeater;
pub mod settings;
pub mod source_control;
pub mod split_view;
pub mod wordlists;

pub use agent::render_agent;
pub use api_mock::render_api_mock;
pub use api_override::render_api_override;
pub use coder::render_coder;
pub use comparer::render_comparer;
pub use crawler::render_crawler;
pub use diagram::render_diagram;
pub use doctor::render_doctor;
pub use editors::ToolEditors;
pub use file_explorer::render_file_explorer;
pub use http::render_http;
pub use intercept::render_intercept;
pub use intruder::render_intruder;
pub use jwt::render_jwt;
pub use notes::render_notes;
pub use port_scanner::render_port_scanner;
pub use r2_storage::render_r2_storage;
pub use repeater::render_repeater;
pub use settings::render_settings;
pub use source_control::render_source_control;
pub use split_view::render_split_view;
pub use wordlists::render_wordlists;

use crate::nav::Page;
use gpui::*;
use halo_bridge::BridgeState;
use halo_debugger::{DebugRegionMeta, region_bounds_sensor};

pub fn render_page(
    page: Page,
    bridge: &BridgeState,
    editors: &ToolEditors,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    let shell_meta = DebugRegionMeta {
        padding: Some("page-shell (full)".into()),
        gap: None,
        notes: Some(format!("page_id={}", page.id())),
    };
    let page_meta = DebugRegionMeta {
        padding: Some("tool body".into()),
        gap: Some("PAGE_GAP=16 / SECTION_GAP=12".into()),
        notes: Some(format!(
            "proxy={} history={}",
            if bridge.proxy.running { "on" } else { "off" },
            bridge.history.len()
        )),
    };
    let page_id = format!("page-{}", page.id());
    let page_label = page.label().to_string();
    let shell_label = format!("{} page", page.label());

    let body = match page {
        // The desktop is the shell, not a tool page.
        Page::Desktop => div().into_any_element(),
        Page::HttpHistory => render_http(bridge, editors, cx).into_any_element(),
        Page::Intercept => render_intercept(bridge, editors, cx).into_any_element(),
        Page::Repeater => render_repeater(bridge, editors, cx).into_any_element(),
        Page::Intruder => render_intruder(bridge, editors, cx).into_any_element(),
        Page::ApiMock => render_api_mock(bridge, editors, cx).into_any_element(),
        Page::ApiOverride => render_api_override(bridge, editors, cx).into_any_element(),
        Page::Jwt => render_jwt(bridge, editors, cx).into_any_element(),
        Page::Comparer => render_comparer(bridge, editors, cx).into_any_element(),
        Page::FileExplorer => render_file_explorer(bridge, editors, cx).into_any_element(),
        Page::R2Storage => render_r2_storage(bridge, editors, cx).into_any_element(),
        Page::Wordlists => render_wordlists(bridge, editors, cx).into_any_element(),
        Page::PortScanner => render_port_scanner(bridge, editors, cx).into_any_element(),
        Page::SplitView => render_split_view(bridge, editors, window, cx).into_any_element(),
        Page::Crawler => render_crawler(bridge, editors, cx).into_any_element(),
        Page::Coder => render_coder(bridge, editors, window, cx).into_any_element(),
        Page::SourceControl => render_source_control(bridge, editors, cx).into_any_element(),
        Page::Agent => render_agent(bridge, editors, window, cx).into_any_element(),
        Page::Notes => render_notes(bridge, editors, window, cx).into_any_element(),
        Page::Diagram => render_diagram(bridge, editors, window, cx).into_any_element(),
        Page::Doctor => render_doctor(bridge, cx).into_any_element(),
        Page::Settings => render_settings(bridge, editors, cx).into_any_element(),
    };

    div()
        .id("page-shell")
        .relative()
        .size_full()
        .min_h_0()
        .child(
            div()
                .relative()
                .size_full()
                .min_h_0()
                .child(body)
                .child(region_bounds_sensor(page_id, page_label, page_meta)),
        )
        .child(region_bounds_sensor("page-shell", shell_label, shell_meta))
}
