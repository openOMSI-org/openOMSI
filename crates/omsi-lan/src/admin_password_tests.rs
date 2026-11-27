use super::*;

/// One player's wrong passwords lock that player out, not the admins; only many sources
/// guessing at once lock everybody.
#[test]
fn wrong_passwords_lock_their_source() {
    let mut adm = ServerAdmin::default();
    for _ in 0..LOCK_AFTER {
        adm.failed("10.0.0.5".into());
    }
    assert!(adm.locked("10.0.0.5"));
    assert!(!adm.locked("10.0.0.6"));
    // (each source counts its share until the lock for all)
    for n in 0..(LOCK_ALL / LOCK_AFTER) {
        for _ in 0..LOCK_AFTER {
            adm.failed(format!("192.0.2.{n}"));
        }
    }
    assert!(adm.locked("10.0.0.6"));
}

/// A wrong answer counts against the player's own address, or their number when they
/// are on this machine (the web gateway's loopback is not one source for all).
#[test]
fn a_wrong_answer_counts_against_its_player() {
    assert_eq!(
        counted_source(Some("203.0.113.9:5000".parse().unwrap()), 1),
        "203.0.113.9"
    );
    assert_eq!(
        counted_source(Some("127.0.0.1:41000".parse().unwrap()), 3),
        "player 3"
    );
    assert_eq!(
        counted_source(Some("127.0.0.1:41001".parse().unwrap()), 4),
        "player 4"
    );
    assert_eq!(counted_source(None, 5), "player 5");
}

/// The challenges come from the secure random: 128 bits, a new one each time.
#[test]
fn challenges_are_fresh() {
    let (a, b) = (fresh_challenge(), fresh_challenge());
    assert_eq!(a.len(), 32);
    assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    assert_ne!(a, b);
}

#[test]
fn the_answer_needs_the_password_and_a_slow_key() {
    let salt = "00112233445566778899aabbccddeeff";
    let a = answer(&format!("c0ffee {salt} {MIN_ROUNDS}"), "correct horse").unwrap();
    assert_eq!(
        a,
        keyed_response(
            &derive("correct horse", &unhex(salt).unwrap(), MIN_ROUNDS),
            "c0ffee"
        )
    );
    assert_ne!(
        answer(&format!("c0ffee {salt} {MIN_ROUNDS}"), "wrong horse"),
        Some(a)
    );
    assert_eq!(answer(&format!("c0ffee {salt} 1"), "correct horse"), None);
    assert_eq!(answer("c0ffee", "correct horse"), None);
    assert_eq!(
        answer(&format!("c0ffee 00 {MIN_ROUNDS}"), "correct horse"),
        None
    );
    assert_eq!(
        answer(
            &format!("c0ffee {salt} {}", MAX_ROUNDS + 1),
            "correct horse"
        ),
        None
    );
}

#[test]
fn the_server_keeps_its_key_until_the_password_changes() {
    let mut adm = ServerAdmin {
        password: "a long password".into(),
        ..Default::default()
    };
    let first = (adm.derived().salt, adm.derived().admin);
    assert_eq!((adm.derived().salt, adm.derived().admin), first);
    adm.password = "another long one".into();
    assert_ne!((adm.derived().salt, adm.derived().admin), first);
}

/// The answer proves the role of the password it was made with, and nothing else: one
/// key for each, with one salt; a moderator's password like the admin's is the admin's.
#[test]
fn the_answer_proves_its_role() {
    let mut adm = ServerAdmin {
        password: "admin secret".into(),
        moderator_password: "mod secret".into(),
        ..Default::default()
    };
    let k = adm.derived();
    let challenge = format!("c0ffee {} {KDF_ROUNDS}", hex(&k.salt));
    let (a, m) = (
        answer(&challenge, "admin secret").unwrap(),
        answer(&challenge, "mod secret").unwrap(),
    );
    assert_eq!(adm.role_of("c0ffee", &a), Some(Role::Admin));
    assert_eq!(adm.role_of("c0ffee", &m), Some(Role::Moderator));
    assert_eq!(
        adm.role_of("c0ffee", &answer(&challenge, "guess").unwrap()),
        None
    );
    // the same answer for another challenge proves nothing
    assert_eq!(adm.role_of("c0ffef", &a), None);
    // no admin password: the moderator's alone
    let mut adm = ServerAdmin {
        moderator_password: "mod secret".into(),
        ..Default::default()
    };
    let challenge = format!("c0ffee {} {KDF_ROUNDS}", hex(&adm.derived().salt));
    assert_eq!(
        adm.role_of("c0ffee", &answer(&challenge, "mod secret").unwrap()),
        Some(Role::Moderator)
    );
    assert_eq!(
        adm.role_of("c0ffee", &answer(&challenge, "").unwrap()),
        None
    );
}
