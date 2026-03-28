use super::*;

fn make_app() -> App {
    App::new(vec!["a".to_string(), "b".to_string(), "c".to_string()])
}

#[tokio::test]
async fn take_count_empty_buf_returns_one() {
    let mut app = make_app();
    assert_eq!(app.take_count(), 1);
}

#[tokio::test]
async fn take_count_single_digit_returns_it() {
    let mut app = make_app();
    app.count_buf = "5".to_string();
    assert_eq!(app.take_count(), 5);
    assert!(app.count_buf.is_empty());
}

#[tokio::test]
async fn take_count_multi_digit_returns_parsed_value() {
    let mut app = make_app();
    app.count_buf = "10".to_string();
    assert_eq!(app.take_count(), 10);
    assert!(app.count_buf.is_empty());
}

#[tokio::test]
async fn take_count_zero_returns_one() {
    let mut app = make_app();
    app.count_buf = "0".to_string();
    assert_eq!(app.take_count(), 1);
}
