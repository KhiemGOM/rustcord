use super::*;

fn team() -> Value {
    json!({ "name": "team", "options": [
        { "type": 1, "name": "add", "options": [{ "type": 6, "name": "who", "required": true }] },
        { "type": 2, "name": "config", "options": [{ "type": 1, "name": "show", "options": [] }] },
    ] })
}

#[test]
fn subcommands_and_groups_are_separate_forms() {
    let c = team();
    let f = forms(&c);
    assert_eq!(f.len(), 2);
    assert_eq!(form_name(&c, &f[0]), "team add");
    assert_eq!(form_name(&c, &f[1]), "team config show");
    assert_eq!(Slash::new(&c, &f[0]).opts.len(), 1);
    assert_eq!(forms(&json!({ "name": "ping", "options": [] })), vec![Vec::<usize>::new()]);
}

#[test]
fn required_options_show_and_optional_ones_wait() {
    let c = json!({ "name": "x", "options": [{ "type": 3, "name": "a", "required": true }, { "type": 3, "name": "b" }] });
    let s = Slash::new(&c, &[]);
    assert_eq!(s.visible(), vec![0]);
    assert_eq!(s.hidden(), vec![1]);
}

#[test]
fn numbers_say_what_range_they_want() {
    let o = json!({ "min_value": 1, "max_value": 10 });
    assert_eq!(typed_range(&o), "Enter a number between 1 and 10.");
    assert!(in_range(&o, 11.0).is_err() && in_range(&o, 0.0).is_err() && in_range(&o, 5.0).is_ok());
    assert_eq!(typed_range(&json!({ "min_value": 2 })), "Enter a number greater than or equal to 2.");
    assert_eq!(typed_range(&json!({ "max_value": 0.5 })), "Enter a number less than or equal to 0.5.");
}

#[test]
fn strings_say_how_long_they_may_be() {
    assert_eq!(chars_msg(&json!({ "min_length": 3, "max_length": 20 })).as_deref(), Some("Enter between 3 and 20 characters."));
    assert_eq!(chars_msg(&json!({ "min_length": 3 })).as_deref(), Some("Enter 3 or more characters."));
    assert_eq!(chars_msg(&json!({ "max_length": 9 })).as_deref(), Some("Enter 9 or fewer characters."));
    assert_eq!(chars_msg(&json!({})), None);
}

#[test]
fn ids_are_taken_from_mentions() {
    assert_eq!(digits("<@123456789012345678>").as_deref(), Some("123456789012345678"));
    assert_eq!(digits("<#123456789012345678>").as_deref(), Some("123456789012345678"));
    assert_eq!(digits("alice"), None);
    assert_eq!(digits("42"), None);
}

#[test]
fn each_kind_has_its_own_complaint() {
    let of = |t: u64| invalid(&json!({ "type": t }));
    assert_eq!(of(4), "Input a valid integer.");
    assert_eq!(of(10), "Input a valid number.");
    assert_eq!(of(6), "Not a valid user.");
    assert_eq!(invalid(&json!({ "type": 3, "choices": [{ "name": "a", "value": "a" }] })), "Not a valid choice.");
}
