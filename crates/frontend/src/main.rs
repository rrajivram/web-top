use leptos::prelude::*;

#[component]
fn App() -> impl IntoView {
    view! {
        <div>
            <h1>"web-top"</h1>
            <p>"macOS System Monitor — loading..."</p>
        </div>
    }
}

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App);
}
