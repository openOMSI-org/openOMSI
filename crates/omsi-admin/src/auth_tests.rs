use super::*;

fn auth() -> Auth {
    Auth::new(
        Passwords {
            admin: "admin-secret".into(),
            moderator: "mod-secret".into(),
        },
        AuthCfg::default(),
    )
}

fn ip(n: u8) -> IpAddr {
    IpAddr::from([192, 0, 2, n])
}

#[test]
fn a_password_gives_its_role_and_a_token() {
    let mut a = auth();
    let now = Instant::now();
    let (t, info) = a.login("admin-secret", ip(1), now).unwrap();
    assert_eq!((info.role, info.permissions.len()), (Role::Admin, 3));
    assert_eq!(t.len(), 64);
    assert_eq!(a.check(&t, now).map(|i| i.role), Some(Role::Admin));
    let (m, info) = a.login("mod-secret", ip(1), now).unwrap();
    assert_eq!(info.role, Role::Moderator);
    assert!(!info.permissions.contains(&Perm::World));
    assert_ne!(t, m);
    assert_eq!(a.login("Admin-secret", ip(1), now), Err(LoginError::Wrong));
    assert_eq!(a.login("", ip(1), now), Err(LoginError::Wrong));
    // (a token that is not one, or the password itself, is no session)
    assert!(a.check("admin-secret", now).is_none());
    assert!(a.check("", now).is_none());
    assert!(a.check(&"f".repeat(64), now).is_none());
}

#[test]
fn no_password_no_door() {
    let mut a = Auth::new(Passwords::default(), AuthCfg::default());
    assert_eq!(
        a.login("", ip(1), Instant::now()),
        Err(LoginError::Disabled)
    );
    assert_eq!(
        a.login("anything", ip(1), Instant::now()),
        Err(LoginError::Disabled)
    );
    assert_eq!(
        format!(
            "{:?}",
            Passwords {
                admin: "hunter22".into(),
                moderator: String::new()
            }
        ),
        "Passwords { admin: true, moderator: false }"
    );
}

#[test]
fn a_session_ends_idle_old_at_logout_or_revoked() {
    let mut a = auth();
    let t0 = Instant::now();
    let (t, _) = a.login("admin-secret", ip(1), t0).unwrap();
    // used every 20 minutes it lives on ...
    let mut now = t0;
    for _ in 0..30 {
        now += Duration::from_secs(20 * 60);
        if now.duration_since(t0) >= Duration::from_secs(12 * 3600) {
            break;
        }
        assert!(a.check(&t, now).is_some(), "{:?}", now.duration_since(t0));
    }
    // ... but not past its longest life
    assert!(
        a.check(&t, t0 + Duration::from_secs(12 * 3600 + 1))
            .is_none()
    );
    // idle for half an hour: over
    let (t, _) = a.login("admin-secret", ip(1), t0).unwrap();
    assert!(a.check(&t, t0 + Duration::from_secs(31 * 60)).is_none());
    // logout and revocation
    let (t, _) = a.login("admin-secret", ip(1), t0).unwrap();
    assert!(a.logout(&t));
    assert!(a.check(&t, t0).is_none());
    assert!(!a.logout(&t));
    let (t, _) = a.login("admin-secret", ip(1), t0).unwrap();
    a.set_passwords(Passwords {
        admin: "a new one!".into(),
        moderator: String::new(),
    });
    assert!(a.check(&t, t0).is_none());
    assert_eq!(a.login("admin-secret", ip(1), t0), Err(LoginError::Wrong));
}

/// A source that keeps guessing waits twice as long each time (an hour at most), until it
/// gets in; an IPv6 client is one source for its whole /64.
#[test]
fn a_source_that_keeps_guessing_waits_longer() {
    let mut a = auth();
    let mut now = Instant::now();
    let mut waits = Vec::new();
    for _ in 0..8 {
        for _ in 0..5 {
            assert_eq!(a.login("nope", ip(1), now), Err(LoginError::Wrong));
        }
        match a.login("admin-secret", ip(1), now) {
            Err(LoginError::Locked { wait, .. }) => waits.push(wait),
            other => panic!("{other:?}"),
        }
        now += Duration::from_secs(waits.last().copied().unwrap_or(0) + 1);
    }
    assert_eq!(&waits[..4], [120, 240, 480, 960]);
    assert!(
        waits.iter().all(|&w| w <= 3600) && waits.last() == Some(&3600),
        "{waits:?}"
    );
    // in at last: the count starts over
    assert!(a.login("admin-secret", ip(1), now).is_ok());
    for _ in 0..5 {
        let _ = a.login("nope", ip(1), now);
    }
    assert!(matches!(
        a.login("admin-secret", ip(1), now),
        Err(LoginError::Locked { wait: 120, .. })
    ));
    // a /64 is one source, whatever address in it
    let v6 = |n: u16| IpAddr::from([0x2001, 0xdb8, 1, 2, n, n, n, n]);
    for n in 0..5 {
        let _ = a.login("nope", v6(n), now);
    }
    assert!(matches!(
        a.login("admin-secret", v6(77), now),
        Err(LoginError::Locked { .. })
    ));
    assert!(
        a.login(
            "admin-secret",
            IpAddr::from([0x2001, 0xdb8, 1, 3, 0, 0, 0, 1]),
            now
        )
        .is_ok()
    );
}

#[test]
fn wrong_passwords_lock_their_source_then_everybody() {
    let mut a = auth();
    let now = Instant::now();
    for _ in 0..5 {
        assert_eq!(a.login("nope", ip(1), now), Err(LoginError::Wrong));
    }
    // locked for that source, even with the right password
    assert!(
        matches!(a.login("admin-secret", ip(1), now), Err(LoginError::Locked { wait, first: true }) if wait > 0 && wait <= 120)
    );
    // (told of once: knocking again is the same lock)
    assert!(matches!(
        a.login("nope", ip(1), now),
        Err(LoginError::Locked { first: false, .. })
    ));
    // another source is still let in
    assert!(a.login("admin-secret", ip(2), now).is_ok());
    // and the lock ends with its window
    assert!(
        a.login("admin-secret", ip(1), now + Duration::from_secs(121))
            .is_ok()
    );
    // wrong passwords from many sources lock the door for all
    let mut a = auth();
    for n in 0..30 {
        let _ = a.login("nope", ip(10 + n), now);
    }
    assert!(matches!(
        a.login("admin-secret", ip(200), now),
        Err(LoginError::Locked { first: true, .. })
    ));
    assert!(matches!(
        a.login("admin-secret", ip(201), now),
        Err(LoginError::Locked { first: false, .. })
    ));
    // but not for this machine itself: a flood from afar keeps no operator out
    let here = IpAddr::from([127, 0, 0, 1]);
    assert!(a.login_from("admin-secret", here, true, now).is_ok());
    // (a loopback address said by a proxy is not this machine: locked like the rest)
    assert!(matches!(
        a.login("admin-secret", here, now),
        Err(LoginError::Locked { .. })
    ));
    // (its own wrong passwords still lock it)
    for _ in 0..5 {
        let _ = a.login_from("nope", here, true, now);
    }
    assert!(matches!(
        a.login_from("admin-secret", here, true, now),
        Err(LoginError::Locked { .. })
    ));
    // a new lock is told of again
    let later = now + Duration::from_secs(121);
    for _ in 0..5 {
        let _ = a.login("nope", ip(1), later);
    }
    assert!(matches!(
        a.login("nope", ip(1), later),
        Err(LoginError::Locked { first: true, .. })
    ));
}

#[test]
fn sessions_are_few() {
    let mut a = auth();
    let now = Instant::now();
    let first = a.login("admin-secret", ip(1), now).unwrap().0;
    for k in 1..=16u64 {
        a.login("admin-secret", ip(1), now + Duration::from_millis(k))
            .unwrap();
    }
    assert_eq!(a.sessions(), 16);
    // (the least lately used made room)
    assert!(a.check(&first, now + Duration::from_secs(1)).is_none());
    // a moderator's logins make room among the moderators only: never an admin's
    let mut a = auth();
    let admins: Vec<String> = (0..10u64)
        .map(|k| {
            a.login("admin-secret", ip(1), now + Duration::from_millis(k))
                .unwrap()
                .0
        })
        .collect();
    let later = now + Duration::from_secs(1);
    for k in 0..40u64 {
        let r = a.login("mod-secret", ip(2), later + Duration::from_millis(k));
        assert!(r.is_ok(), "{k}: {r:?}");
    }
    assert!(
        admins
            .iter()
            .all(|t| a.check(t, later + Duration::from_secs(1)).is_some())
    );
    assert_eq!(a.sessions(), 16);
    // all taken by admins: a moderator waits, an admin still gets in
    let mut a = auth();
    for k in 0..16u64 {
        a.login("admin-secret", ip(1), now + Duration::from_millis(k))
            .unwrap();
    }
    assert_eq!(a.login("mod-secret", ip(2), later), Err(LoginError::Full));
    assert!(a.login("admin-secret", ip(1), later).is_ok());
}
