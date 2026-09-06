// Windows のリリースビルドでコンソールウィンドウを表示しない
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    b_listener_lib::run()
}
