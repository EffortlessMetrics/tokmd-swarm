mod scope { mod left { fn repeat() {} } mod right { fn repeat() {} } }

struct First;
struct Second;

impl First {
    fn repeat(&self) {}
}

impl Second {
    fn repeat(&self) {}
}

fn outer() {
    fn nested() {}
    /* café */ fn éclair() {}
    let _ = r#"
fn ghost() {}
"#;
}
