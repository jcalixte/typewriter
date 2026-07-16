use super::*;

fn type_str(w: &mut Wizard, s: &str) {
    for c in s.chars() {
        assert!(w.key(Key::Char(c)).is_empty());
    }
}

fn repos() -> Vec<RepoChoice> {
    vec![
        RepoChoice {
            full_name: "you/big-notes".into(),
            size_kb: 562 * 1024,
        },
        RepoChoice {
            full_name: "you/notes".into(),
            size_kb: 420,
        },
        RepoChoice {
            full_name: "you/dotfiles".into(),
            size_kb: 90,
        },
    ]
}

/// The whole first-boot happy path, effect by effect.
#[test]
fn first_boot_happy_path() {
    let mut w = Wizard::first_boot();
    assert_eq!(w.pending(), None); // starts editing, nothing to execute

    // Wi-Fi: ssid, Enter, pass, Enter → TestWifi.
    type_str(&mut w, "MyNet");
    assert!(w.key(Key::Enter).is_empty());
    type_str(&mut w, "hunter2");
    let fx = w.key(Key::Enter);
    assert_eq!(
        fx,
        vec![Effect::TestWifi {
            ssid: "MyNet".into(),
            pass: "hunter2".into()
        }]
    );

    // Join ok → conf persisted + device flow starts.
    let fx = w.event(Event::WifiOk);
    assert_eq!(fx.len(), 2);
    assert!(matches!(&fx[0], Effect::WriteConf(c) if c.wifi_ssid == "MyNet"));
    assert_eq!(fx[1], Effect::StartAuth);

    // Code arrives → no effect (driver keeps polling), QR screen shown.
    let fx = w.event(Event::AuthCode {
        verification_uri: "https://github.com/login/device".into(),
        user_code: "ABCD-1234".into(),
    });
    assert!(fx.is_empty());

    // Token granted → conf persisted (token + identity) + repo listing.
    let fx = w.event(Event::AuthDone {
        token: "ghu_tok".into(),
        login: "you".into(),
        name: String::new(),
        email: String::new(),
    });
    assert_eq!(fx.len(), 2);
    match &fx[0] {
        Effect::WriteConf(c) => {
            assert_eq!(c.token, "ghu_tok");
            assert_eq!(c.gh_user, "you");
            assert_eq!(c.author_name, "you"); // blank name falls back to login
            assert_eq!(c.author_email, "you@users.noreply.github.com");
        }
        other => panic!("expected WriteConf, got {other:?}"),
    }
    assert_eq!(fx[1], Effect::FetchRepos);

    // Pick "you/notes" (under the gate) → conf carries the remote + Clone.
    assert!(w.event(Event::Repos(repos())).is_empty());
    type_str(&mut w, "notes");
    // Filter "notes" matches big-notes then notes; move to the second row.
    assert!(w.key(Key::Down).is_empty());
    let fx = w.key(Key::Enter);
    assert_eq!(fx.len(), 2);
    match &fx[0] {
        Effect::WriteConf(c) => {
            assert_eq!(c.remote_url, "https://github.com/you/notes.git");
        }
        other => panic!("expected WriteConf, got {other:?}"),
    }
    assert_eq!(
        fx[1],
        Effect::Clone {
            full_name: "you/notes".into()
        }
    );

    // Clone progress + done → final conf write, then any key finishes.
    assert!(w.event(Event::CloneProgress("12/340 files".into())).is_empty());
    let fx = w.event(Event::CloneDone);
    assert!(matches!(&fx[0], Effect::WriteConf(_)));
    assert_eq!(w.key(Key::Enter), vec![Effect::Finish]);
}

#[test]
fn size_gate_refuses_and_allows_repick() {
    let mut w = Wizard::first_boot();
    // Jump straight to the pick screen.
    w.event(Event::Repos(repos()));
    type_str(&mut w, "big");
    let fx = w.key(Key::Enter);
    assert!(fx.is_empty(), "over-gate pick must not clone: {fx:?}");
    // The refusal shows, and a fresh filter + pick still works.
    let mut f = Frame::new_white();
    w.draw_into(&mut f); // must not panic with the refusal line up
    w.key(Key::DeleteLine);
    type_str(&mut w, "dotfiles");
    let fx = w.key(Key::Enter);
    assert_eq!(
        fx.last(),
        Some(&Effect::Clone {
            full_name: "you/dotfiles".into()
        })
    );
}

#[test]
fn wifi_failure_returns_to_password_edit() {
    let mut w = Wizard::first_boot();
    type_str(&mut w, "MyNet");
    w.key(Key::Enter);
    type_str(&mut w, "wrong");
    w.key(Key::Enter);
    assert!(w.event(Event::WifiFailed("timeout".into())).is_empty());
    // Editing the password again: fix it and re-test.
    for _ in 0..5 {
        w.key(Key::Backspace);
    }
    type_str(&mut w, "right");
    let fx = w.key(Key::Enter);
    assert_eq!(
        fx,
        vec![Effect::TestWifi {
            ssid: "MyNet".into(),
            pass: "right".into()
        }]
    );
}

#[test]
fn resume_skips_satisfied_steps() {
    // Conf with Wi-Fi but no token → resumes at sign-in.
    let mut c = conf::Conf::default();
    c.wifi_ssid = "MyNet".into();
    c.wifi_pass = "hunter2".into();
    let w = Wizard::resume(c.clone());
    assert_eq!(w.pending(), Some(Effect::StartAuth));

    // Full conf but no repo (power-pull mid-clone) → resumes at repo pick.
    c.token = "ghu_tok".into();
    c.gh_user = "you".into();
    let w = Wizard::resume(c);
    assert_eq!(w.pending(), Some(Effect::FetchRepos));
}

#[test]
fn auth_failure_restarts_flow_and_esc_requests_fresh_code() {
    let mut w = Wizard::resume({
        let mut c = conf::Conf::default();
        c.wifi_ssid = "MyNet".into();
        c
    });
    assert_eq!(w.pending(), Some(Effect::StartAuth));
    // A failure parks on the retry screen (no auto-retry loop) until Enter.
    assert_eq!(w.event(Event::AuthFailed("expired".into())), vec![]);
    assert!(w.key(Key::Char('x')).is_empty()); // random keys don't retry
    assert_eq!(w.key(Key::Enter), vec![Effect::StartAuth]);
    w.event(Event::AuthCode {
        verification_uri: "https://github.com/login/device".into(),
        user_code: "ABCD-1234".into(),
    });
    assert_eq!(w.key(Key::Escape), vec![Effect::StartAuth]);
}

#[test]
fn open_network_allows_empty_password() {
    let mut w = Wizard::first_boot();
    type_str(&mut w, "OpenNet");
    w.key(Key::Enter);
    let fx = w.key(Key::Enter); // empty password committed
    assert_eq!(
        fx,
        vec![Effect::TestWifi {
            ssid: "OpenNet".into(),
            pass: String::new()
        }]
    );
}

#[test]
fn backspace_past_empty_password_returns_to_ssid() {
    let mut w = Wizard::first_boot();
    type_str(&mut w, "MyNet");
    w.key(Key::Enter);
    w.key(Key::Backspace); // empty pass → back on SSID
    w.key(Key::Backspace); // now eats the SSID's last char
    w.key(Key::Enter); // Enter on "MyNe" → to password again
    type_str(&mut w, "p");
    let fx = w.key(Key::Enter);
    assert_eq!(
        fx,
        vec![Effect::TestWifi {
            ssid: "MyNe".into(),
            pass: "p".into()
        }]
    );
}

#[test]
fn repos_failure_parks_then_enter_retries_and_backspace_edits_wifi() {
    let mut w = Wizard::resume({
        let mut c = conf::Conf::default();
        c.wifi_ssid = "MyNet".into();
        c.token = "ghu_tok".into();
        c
    });
    assert_eq!(w.pending(), Some(Effect::FetchRepos));
    assert_eq!(w.event(Event::ReposFailed("500".into())), vec![]);
    assert_eq!(w.key(Key::Enter), vec![Effect::FetchRepos]);
    // And the auth-retry escape hatch back to Wi-Fi:
    w.event(Event::AuthFailed("dead network".into()));
    assert!(w.key(Key::Backspace).is_empty());
    // Now editing the SSID again.
    assert!(w.key(Key::Char('X')).is_empty());
}

#[test]
fn clone_failure_reloads_the_list() {
    let mut w = Wizard::first_boot();
    w.event(Event::Repos(repos()));
    type_str(&mut w, "dotfiles");
    w.key(Key::Enter);
    assert_eq!(
        w.event(Event::CloneFailed("TLS".into())),
        vec![Effect::FetchRepos]
    );
}

/// The QR screen really draws modules into the reserved square (the encoder
/// ran and the scale fit), and the quiet zone above the QR stays white.
#[test]
fn qr_renders_modules() {
    let mut w = Wizard::resume({
        let mut c = conf::Conf::default();
        c.wifi_ssid = "MyNet".into();
        c
    });
    w.event(Event::AuthCode {
        verification_uri: "https://github.com/login/device".into(),
        user_code: "ABCD-1234".into(),
    });
    let mut f = Frame::new_white();
    w.draw_into(&mut f);
    // The 200px box sits at x = WIDTH-200, y = 40. Any non-white byte in its
    // row band beyond the text column proves modules landed.
    let x_byte0 = (display::WIDTH as usize - 200) / 8;
    let inked = (40..240)
        .flat_map(|y| (x_byte0..display::FB_BYTES_W).map(move |xb| (y, xb)))
        .filter(|(y, xb)| f.bytes()[y * display::FB_BYTES_W + xb] != 0xFF)
        .count();
    assert!(inked > 50, "expected QR modules in the box, found {inked} inked bytes");
    // Top rows of the panel above the box (y < 8) stay white in that column
    // band — the quiet zone / layout didn't smear upward.
    let smear = (0..8)
        .flat_map(|y| (x_byte0..display::FB_BYTES_W).map(move |xb| (y, xb)))
        .filter(|(y, xb)| f.bytes()[y * display::FB_BYTES_W + xb] != 0xFF)
        .count();
    assert_eq!(smear, 0);
}

/// Tab toggles password visibility: it never types a tab into the password,
/// and the rendered field actually changes between cleartext and mask.
#[test]
fn tab_toggles_password_reveal() {
    let mut w = Wizard::first_boot();
    type_str(&mut w, "MyNet");
    w.key(Key::Enter); // onto the password field
    type_str(&mut w, "abc");
    assert_eq!(w.conf().wifi_pass, "abc");

    // Default is shown. Snapshot, hide with Tab, snapshot again.
    let mut shown = Frame::new_white();
    w.draw_into(&mut shown);
    assert!(w.key(Key::Char('\t')).is_empty()); // Tab is not text…
    assert_eq!(w.conf().wifi_pass, "abc"); // …and leaves the password alone
    let mut hidden = Frame::new_white();
    w.draw_into(&mut hidden);
    assert_ne!(
        shown.bytes(),
        hidden.bytes(),
        "hiding the password must change the render"
    );

    // Toggling back reproduces the cleartext render exactly.
    w.key(Key::Char('\t'));
    let mut reshown = Frame::new_white();
    w.draw_into(&mut reshown);
    assert_eq!(shown.bytes(), reshown.bytes());
}

/// Every screen renders without panicking (layout arithmetic guard).
#[test]
fn all_screens_draw() {
    let mut f = Frame::new_white();
    let mut w = Wizard::first_boot();
    w.draw_into(&mut f);
    type_str(&mut w, "MyNet");
    w.key(Key::Enter);
    w.draw_into(&mut f);
    w.key(Key::Enter);
    w.draw_into(&mut f); // testing
    w.event(Event::WifiOk);
    w.draw_into(&mut f); // auth starting
    w.event(Event::AuthCode {
        verification_uri: "https://github.com/login/device".into(),
        user_code: "ABCD-1234".into(),
    });
    w.draw_into(&mut f); // QR screen
    w.event(Event::AuthDone {
        token: "t".into(),
        login: "you".into(),
        name: "You".into(),
        email: "you@example.com".into(),
    });
    w.draw_into(&mut f); // repo loading
    w.event(Event::Repos(repos()));
    w.draw_into(&mut f); // pick list
    w.key(Key::Enter);
    w.draw_into(&mut f); // cloning (first repo is over-gate → refused, still picks screen)
    w.event(Event::CloneProgress("downloading".into()));
    w.draw_into(&mut f);
    w.event(Event::CloneDone);
    w.draw_into(&mut f); // done
}
