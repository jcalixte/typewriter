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
    assert_eq!(w.pending(), Some(Effect::ScanWifi)); // scans for networks first

    // Scan → pick "MyNet" from the list → type the password → TestWifi.
    assert!(w
        .event(Event::WifiScan(vec!["MyNet".into(), "OtherNet".into()]))
        .is_empty());
    assert!(w.key(Key::Enter).is_empty()); // pick first row (MyNet) → password
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
    w.event(Event::WifiScan(vec!["MyNet".into()]));
    w.key(Key::Enter); // pick MyNet → password
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
    w.event(Event::WifiScan(vec!["OpenNet".into()]));
    w.key(Key::Enter); // pick OpenNet → password
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
fn manual_entry_navigates_ssid_and_password() {
    let mut w = Wizard::first_boot();
    w.event(Event::WifiScan(vec![])); // no networks found
    w.key(Key::Escape); // type it manually → SSID field (seeded empty)
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

/// Scan → filter → pick a specific row → the chosen SSID is what gets joined.
#[test]
fn scan_filter_and_pick_selects_the_row() {
    let mut w = Wizard::first_boot();
    w.event(Event::WifiScan(vec![
        "Home-2G".into(),
        "Home-5G".into(),
        "Cafe".into(),
    ]));
    type_str(&mut w, "home"); // filters to the two Home networks
    w.key(Key::Down); // move to "Home-5G"
    w.key(Key::Enter); // pick it → password
    let fx = w.key(Key::Enter); // empty password
    assert_eq!(
        fx,
        vec![Effect::TestWifi {
            ssid: "Home-5G".into(),
            pass: String::new()
        }]
    );
}

/// An empty scan (or a filter that matches nothing) rescans on Enter rather
/// than dead-ending.
#[test]
fn empty_scan_rescans_on_enter() {
    let mut w = Wizard::first_boot();
    w.event(Event::WifiScan(vec![]));
    assert_eq!(w.key(Key::Enter), vec![Effect::ScanWifi]);
    // A live filter with no match also rescans.
    w.event(Event::WifiScan(vec!["Cafe".into()]));
    type_str(&mut w, "zzz");
    assert_eq!(w.key(Key::Enter), vec![Effect::ScanWifi]);
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
    w.event(Event::WifiScan(vec!["MyNet".into()]));
    w.key(Key::Enter); // pick MyNet → password field
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
    w.draw_into(&mut f); // scanning
    w.event(Event::WifiScan(vec!["MyNet".into(), "OtherNet".into()]));
    w.draw_into(&mut f); // pick list
    w.key(Key::Escape); // manual SSID entry
    w.draw_into(&mut f); // WifiEdit field 0
    type_str(&mut w, "MyNet");
    w.key(Key::Enter);
    w.draw_into(&mut f); // WifiEdit password
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

    // The reset menu and its confirm/progress screens (only reached via `:setup`).
    let mut s = Wizard::setup(full_conf(), true);
    s.draw_into(&mut f); // reset menu
    s.key(Key::Down); // GitHub (row 1)
    s.key(Key::Down); // Notes repo (row 2)
    s.key(Key::Down); // Factory reset (row 3)
    s.key(Key::Enter); // → confirm screen
    s.draw_into(&mut f); // ConfirmWipe (dirty warning shown)
    type_str(&mut s, "erase");
    s.key(Key::Enter); // → Wiping
    s.draw_into(&mut f); // Wiping

    // The repo-switch confirmation (reset mode, clean card so the switch isn't
    // gated): Notes repo → list → pick a different repo → confirm.
    let mut r = Wizard::setup(full_conf(), false);
    r.key(Key::Down); // GitHub
    r.key(Key::Down); // Notes repo
    r.key(Key::Enter); // → RepoLoading
    r.event(Event::Repos(repos()));
    type_str(&mut r, "dotfiles"); // filter to a different repo
    r.key(Key::Enter); // → ConfirmRepoSwitch
    r.draw_into(&mut f); // ConfirmRepoSwitch
}

/// A fully-provisioned conf, as `:setup` would be handed at boot.
fn full_conf() -> conf::Conf {
    let mut c = conf::Conf::default();
    c.wifi_ssid = "MyNet".into();
    c.wifi_pass = "hunter2".into();
    c.token = "ghu_tok".into();
    c.gh_user = "you".into();
    c.author_name = "You".into();
    c.author_email = "you@example.com".into();
    c.remote_url = "https://github.com/you/notes.git".into();
    c
}

#[test]
fn setup_menu_done_finishes_without_touching_conf() {
    let mut w = Wizard::setup(full_conf(), false);
    // Opens on the menu, nothing pending (waits for a choice).
    assert_eq!(w.pending(), None);
    // Down to "Done" (row 4: Wi-Fi, GitHub, Notes repo, Factory reset, Done),
    // Enter → Finish, no WriteConf (backing out is harmless — nothing changed).
    w.key(Key::Down);
    w.key(Key::Down);
    w.key(Key::Down);
    w.key(Key::Down);
    assert_eq!(w.key(Key::Enter), vec![Effect::Finish]);
}

#[test]
fn setup_menu_wifi_reruns_scan_then_returns_to_menu() {
    let mut w = Wizard::setup(full_conf(), false);
    // Row 0 = Wi-Fi → rescan.
    assert_eq!(w.key(Key::Enter), vec![Effect::ScanWifi]);
    w.event(Event::WifiScan(vec!["NewNet".into()]));
    w.key(Key::Enter); // pick NewNet → password field
    type_str(&mut w, "pw");
    // Enter on the password tests Wi-Fi…
    assert_eq!(w.key(Key::Enter), vec![Effect::TestWifi {
        ssid: "NewNet".into(),
        pass: "pw".into(),
    }]);
    // …and a good join persists the conf and lands back on the menu (NOT the
    // linear sign-in step), because the token is already set.
    let fx = w.event(Event::WifiOk);
    assert!(matches!(fx.as_slice(), [Effect::WriteConf(_)]), "got {fx:?}");
    // Back on the menu: Done (row 4) finishes.
    w.key(Key::Down);
    w.key(Key::Down);
    w.key(Key::Down);
    w.key(Key::Down);
    assert_eq!(w.key(Key::Enter), vec![Effect::Finish]);
}

#[test]
fn setup_menu_reauth_updates_token_then_returns_to_menu() {
    let mut w = Wizard::setup(full_conf(), false);
    // Row 1 = GitHub account → device flow.
    w.key(Key::Down);
    assert_eq!(w.key(Key::Enter), vec![Effect::StartAuth]);
    w.event(Event::AuthCode {
        verification_uri: "https://github.com/login/device".into(),
        user_code: "ABCD-1234".into(),
    });
    let fx = w.event(Event::AuthDone {
        token: "ghu_new".into(),
        login: "you".into(),
        name: "You".into(),
        email: "you@example.com".into(),
    });
    // Re-auth in reset mode persists and returns to the menu — it does NOT walk
    // on to the repo pick (the repo is unchanged).
    assert!(matches!(fx.as_slice(), [Effect::WriteConf(c)] if c.token == "ghu_new"), "got {fx:?}");
    assert_eq!(w.pending(), None); // on the menu, waiting
}

/// Navigate the reset menu to the factory-reset confirmation screen.
fn to_confirm_wipe(dirty: bool) -> Wizard {
    let mut w = Wizard::setup(full_conf(), dirty);
    w.key(Key::Down); // GitHub account (row 1)
    w.key(Key::Down); // Notes repo (row 2)
    w.key(Key::Down); // Factory reset (row 3)
    assert!(w.key(Key::Enter).is_empty(), "opening confirm emits no effect");
    assert_eq!(w.pending(), None); // waits for the typed word
    w
}

#[test]
fn factory_reset_confirms_then_emits_wipe() {
    let mut w = to_confirm_wipe(false);
    // A wrong word does not wipe — it stays on the confirm screen.
    type_str(&mut w, "nope");
    assert!(w.key(Key::Enter).is_empty(), "wrong word must not wipe");
    // Clear it and type the real word (case-insensitive) → the wipe fires.
    w.key(Key::DeleteLine);
    type_str(&mut w, "ERASE");
    assert_eq!(w.key(Key::Enter), vec![Effect::FactoryReset]);
}

#[test]
fn factory_reset_esc_returns_to_menu() {
    let mut w = to_confirm_wipe(false);
    type_str(&mut w, "er");
    w.key(Key::Escape); // cancel back to the menu (on the Factory-reset row)
    // Proof we're back on the menu: Down lands on Done, Enter finishes.
    w.key(Key::Down);
    assert_eq!(w.key(Key::Enter), vec![Effect::Finish]);
}

#[test]
fn factory_reset_backspace_past_empty_cancels() {
    let mut w = to_confirm_wipe(false);
    // Nothing typed yet: the first Backspace cancels back to the menu (on the
    // Factory-reset row 3); Down lands on Done (row 4).
    assert!(w.key(Key::Backspace).is_empty());
    w.key(Key::Down);
    assert_eq!(w.key(Key::Enter), vec![Effect::Finish]);
}

#[test]
fn factory_reset_confirm_warns_louder_when_dirty() {
    // The dirty warning line is drawn only when the card carries unpushed
    // work — the two renders must differ.
    let clean = to_confirm_wipe(false);
    let dirty = to_confirm_wipe(true);
    let (mut fc, mut fd) = (Frame::new_white(), Frame::new_white());
    clean.draw_into(&mut fc);
    dirty.draw_into(&mut fd);
    assert_ne!(fc.bytes(), fd.bytes(), "dirty confirm must show an extra warning");
}

#[test]
fn wipe_failed_returns_to_menu_with_notice() {
    let mut w = to_confirm_wipe(false);
    type_str(&mut w, "erase");
    assert_eq!(w.key(Key::Enter), vec![Effect::FactoryReset]);
    // The driver reports a failed delete → back on the menu (Factory-reset row
    // 3) to retry; Down lands on Done (row 4).
    assert!(w.event(Event::WipeFailed("FR_DENIED".into())).is_empty());
    w.key(Key::Down);
    assert_eq!(w.key(Key::Enter), vec![Effect::Finish]);
}

/// Reset menu → Notes repo (clean card) → the repo pick list loaded.
fn to_setup_repo_pick() -> Wizard {
    let mut w = Wizard::setup(full_conf(), false);
    w.key(Key::Down); // GitHub account (row 1)
    w.key(Key::Down); // Notes repo (row 2)
    assert_eq!(w.key(Key::Enter), vec![Effect::FetchRepos]);
    w.event(Event::Repos(repos()));
    w
}

#[test]
fn setup_repo_switch_dirty_guard_refuses() {
    // Unpushed work on the card: a switch (which deletes the working copy)
    // must refuse rather than lose it. Stays on the menu, nothing pending.
    let mut w = Wizard::setup(full_conf(), true);
    w.key(Key::Down); // GitHub
    w.key(Key::Down); // Notes repo
    assert!(w.key(Key::Enter).is_empty(), "a dirty card must not start a switch");
    assert_eq!(w.pending(), None); // still on the menu, no FetchRepos
}

#[test]
fn setup_repo_switch_clean_lists_repos() {
    let mut w = Wizard::setup(full_conf(), false);
    w.key(Key::Down); // GitHub
    w.key(Key::Down); // Notes repo
    assert_eq!(w.key(Key::Enter), vec![Effect::FetchRepos]);
}

#[test]
fn setup_repo_switch_same_repo_is_noop() {
    // Re-choosing the repo already on the card doesn't delete or re-clone it.
    let mut w = to_setup_repo_pick();
    type_str(&mut w, "you/notes"); // the current repo (matches conf.remote_url)
    assert!(w.key(Key::Enter).is_empty(), "same repo must not clone");
    assert_eq!(w.pending(), None); // back on the menu, no delete/clone
    assert_eq!(w.conf().remote_url, "https://github.com/you/notes.git"); // untouched
}

#[test]
fn setup_repo_switch_esc_cancels() {
    let mut w = to_setup_repo_pick();
    type_str(&mut w, "dotfiles");
    w.key(Key::Enter); // → ConfirmRepoSwitch
    assert!(w.key(Key::Escape).is_empty(), "Esc cancels the switch");
    assert_eq!(w.pending(), None); // back on the menu
    assert_eq!(w.conf().remote_url, "https://github.com/you/notes.git"); // not committed
}

#[test]
fn setup_repo_switch_needs_the_repo_name_typed() {
    // The switch only fires once the target repo's short name is typed — a wrong
    // word is refused, so a stray Enter can't wipe the working copy.
    let mut w = to_setup_repo_pick();
    type_str(&mut w, "dotfiles");
    w.key(Key::Enter); // → ConfirmRepoSwitch
    type_str(&mut w, "notes"); // the *current* repo's name, not the target
    assert!(w.key(Key::Enter).is_empty(), "wrong word must not switch");
    assert_eq!(w.conf().remote_url, "https://github.com/you/notes.git"); // untouched
    w.key(Key::DeleteLine); // clear the field
    type_str(&mut w, "DotFiles"); // case-insensitive, matches you/dotfiles
    let fx = w.key(Key::Enter);
    assert_eq!(fx[0], Effect::DeleteRepo);
    assert_eq!(fx.last(), Some(&Effect::Clone { full_name: "you/dotfiles".into() }));
}

#[test]
fn setup_repo_pick_esc_returns_to_menu() {
    // Browsing the repo list, then Esc without picking → back to the menu.
    let mut w = to_setup_repo_pick();
    assert!(w.key(Key::Escape).is_empty());
    assert_eq!(w.pending(), None);
    w.key(Key::Down); // Factory reset (row 3)
    w.key(Key::Down); // Done (row 4)
    assert_eq!(w.key(Key::Enter), vec![Effect::Finish]);
}

#[test]
fn setup_repo_switch_different_repo_confirms_then_clones() {
    let mut w = to_setup_repo_pick();
    type_str(&mut w, "dotfiles"); // a different repo
    // Picking it opens the confirmation (no effect yet — a switch is destructive).
    assert!(w.key(Key::Enter).is_empty(), "a switch is confirmed first");
    // Typed-word guard: a bare Enter over the empty field does nothing.
    assert!(w.key(Key::Enter).is_empty(), "an empty confirm must not switch");
    // Type the target repo's name, then Enter → delete the old tree, persist the
    // new conf, clone the new tip.
    type_str(&mut w, "dotfiles");
    let fx = w.key(Key::Enter);
    assert_eq!(fx.len(), 3);
    assert_eq!(fx[0], Effect::DeleteRepo);
    match &fx[1] {
        Effect::WriteConf(c) => {
            assert_eq!(c.remote_url, "https://github.com/you/dotfiles.git");
        }
        other => panic!("expected WriteConf, got {other:?}"),
    }
    assert_eq!(fx[2], Effect::Clone { full_name: "you/dotfiles".into() });
    // Clone done → back to the reset menu (like the other sub-flows), conf written.
    let fx = w.event(Event::CloneDone);
    assert!(matches!(fx.as_slice(), [Effect::WriteConf(_)]), "got {fx:?}");
    assert_eq!(w.pending(), None); // on the menu

    // The switched repo is now the on-disk one: re-picking it no-ops.
    assert_eq!(w.key(Key::Enter), vec![Effect::FetchRepos]); // Notes repo row again
    w.event(Event::Repos(repos()));
    type_str(&mut w, "dotfiles");
    assert!(w.key(Key::Enter).is_empty(), "the new repo is now on disk");
    assert_eq!(w.pending(), None);
}

#[test]
fn setup_repo_switch_failed_clone_avoids_noop_trap() {
    // A switch whose clone fails already deleted the old tree, so the card has
    // no valid repo. Re-picking the *same* target must switch again (delete +
    // clone), never no-op back onto a repo that isn't there.
    let mut w = to_setup_repo_pick();
    type_str(&mut w, "dotfiles");
    w.key(Key::Enter); // → ConfirmRepoSwitch
    type_str(&mut w, "dotfiles"); // confirm word
    let fx = w.key(Key::Enter); // confirm
    assert_eq!(fx[0], Effect::DeleteRepo);
    // Clone fails → back to the pick list.
    assert_eq!(w.event(Event::CloneFailed("TLS".into())), vec![Effect::FetchRepos]);
    w.event(Event::Repos(repos()));
    // Re-pick the very repo we were switching to: NOT on disk, so a fresh switch.
    type_str(&mut w, "dotfiles");
    assert!(w.key(Key::Enter).is_empty(), "→ confirm, not a menu no-op");
    type_str(&mut w, "dotfiles"); // confirm word again
    let fx = w.key(Key::Enter); // confirm again → the switch effects
    assert_eq!(fx[0], Effect::DeleteRepo);
    assert_eq!(fx.last(), Some(&Effect::Clone { full_name: "you/dotfiles".into() }));
}

#[test]
fn setup_done_refused_when_switch_incomplete() {
    // After a failed switch (no working copy on disk), Done must refuse — it
    // would boot a card whose repo is missing. The user must finish a clone.
    let mut w = to_setup_repo_pick();
    type_str(&mut w, "dotfiles");
    w.key(Key::Enter); // → ConfirmRepoSwitch
    type_str(&mut w, "dotfiles"); // confirm word
    w.key(Key::Enter); // confirm → effects
    w.event(Event::CloneFailed("TLS".into())); // repo deleted, clone failed
    w.event(Event::Repos(repos())); // land in the pick list
    w.key(Key::Escape); // Esc back to the menu (repo_on_disk is now None)
    w.key(Key::Down); // Factory reset (row 3)
    w.key(Key::Down); // Done (row 4)
    assert!(w.key(Key::Enter).is_empty(), "Done must refuse with no working copy");
}

#[test]
fn wrap_words_keeps_lines_within_width() {
    // The actual size-gate message shape — must fit ≤3 wrapped lines at 74.
    let msg = "you/big-notes is 562 MB - too large to set up from the device. \
               Pick or create a smaller repo, or seed the card from a computer \
               once (typoena.dev).";
    let lines = wrap_words(msg, 74);
    assert!(lines.len() <= 3, "should wrap to ≤3 lines, got {lines:?}");
    for l in &lines {
        assert!(l.chars().count() <= 74, "line over 74 chars: {l:?}");
    }
    // No word is lost or split across the join.
    assert_eq!(lines.join(" "), msg.split_whitespace().collect::<Vec<_>>().join(" "));
}

#[test]
fn wrap_words_hard_splits_an_overlong_word() {
    let long = "a".repeat(200);
    let lines = wrap_words(&long, 74);
    assert_eq!(lines.len(), 3); // 74 + 74 + 52
    assert!(lines.iter().all(|l| l.chars().count() <= 74));
    assert_eq!(lines.concat(), long);
}

/// A blank card the person brought opens on the consent gate and waits for a
/// key — nothing is scanned or touched until they accept.
#[test]
fn blank_card_gates_on_consent() {
    let w = Wizard::adopt_blank_card();
    // Unlike `first_boot()` (which pends ScanWifi), consent waits for a choice.
    assert_eq!(w.pending(), None);
    // And it renders without panicking.
    let mut f = Frame::new_white();
    w.draw_into(&mut f);
}

/// Accepting the consent erases the whole card, then — once the driver reports
/// the wipe done — walks into the normal Wi-Fi scan.
#[test]
fn consent_accept_wipes_then_starts_wifi() {
    let mut w = Wizard::adopt_blank_card();
    assert_eq!(w.key(Key::Enter), vec![Effect::WipeCard]);
    // Driver finished the wipe → the linear flow begins at Wi-Fi.
    assert_eq!(w.event(Event::WipeCardDone), vec![Effect::ScanWifi]);
}

/// Declining leaves the card untouched: the wizard asks the driver to abort,
/// and (crucially) emits no wipe/write effect.
#[test]
fn consent_decline_aborts() {
    let mut w = Wizard::adopt_blank_card();
    assert_eq!(w.key(Key::Escape), vec![Effect::Decline]);
}

/// A failed wipe drops back to the consent screen (first-boot mode), not the
/// `:setup` reset menu — proven by the consent Enter still emitting WipeCard.
#[test]
fn consent_wipe_failure_returns_to_consent() {
    let mut w = Wizard::adopt_blank_card();
    assert_eq!(w.key(Key::Enter), vec![Effect::WipeCard]);
    assert!(w.event(Event::WipeFailed("FR_DENIED".into())).is_empty());
    // Back on consent: Enter retries the wipe (a menu would emit nothing here).
    assert_eq!(w.key(Key::Enter), vec![Effect::WipeCard]);
}

/// The consent screen and the erase-in-progress screen both render.
#[test]
fn consent_screens_draw() {
    let mut f = Frame::new_white();
    let mut w = Wizard::adopt_blank_card();
    w.draw_into(&mut f); // consent
    w.key(Key::Enter); // → Wiping
    w.draw_into(&mut f); // erasing
}
