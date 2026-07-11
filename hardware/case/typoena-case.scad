// ============================================================================
//  Typoena — 3D-printed enclosure  ·  "typewriter body"  ·  rev v0 (concept)
// ----------------------------------------------------------------------------
//  A shallow sage wedge. The e-paper strip sits on a reclined deck where a
//  typewriter's sheet of paper would be; the keyboard you bring rests in front.
//  No platen part (keeps the print simple) — the rounded back-top edge is a
//  subtle roll that nods to one for free.
//
//  Everything here is PARAMETRIC. Numbers that come from a datasheet are noted;
//  numbers marked  << MEASURE >>  are best-guesses you must confirm against the
//  real board / breakout before printing a final.
//
//  Units: millimetres.   Render:  see hardware/case/README.md
//
//  Parts (set `show` below):
//    "assembled"   – everything in place, coloured (screen ghosted in)
//    "body"        – the shell only (print deck-up or on its back)
//    "bracket"     – the screen retaining frame (print flat)
//    "baseplate"   – the chassis / bottom cover (print flat)
//    "print_plate" – all printed parts laid out side by side
//    "section"     – midline cross-section: how the screen is trapped
// ============================================================================

show = "section";
$fn = 48;

// ---- body envelope --------------------------------------------------------
W        = 176;   // width  (X)  — screen 150.9 + bezel + walls
D        = 104;   // depth  (Y)  — front (keyboard) .. back (ports)
Hf       = 24;    // height at the FRONT edge
Hb       = 58;    // height at the BACK edge  (Hf<Hb makes the reclined deck)
wall     = 2.4;   // side/back wall thickness
top_wall = 2.6;   // deck thickness (before the bezel lip is cut into it)
corner_r = 8;     // rounded vertical + top-edge radius (the "machined" look)

// deck slope, derived from the pillar centres (this is the *true* top plane)
theta    = atan((Hb - Hf) / (D - 2*corner_r));   // ~21 deg with the defaults
// >> THE ergonomics dial. Raise Hb for a more vertical, easier-to-read screen;
//    lower it for a flatter, more typewriter-like deck. 18-22 deg = shallow,
//    28-35 deg reads better when you're sitting close.

// ---- e-paper panel : GDEY0579T93 (datasheet) ------------------------------
G_w  = 150.92;  G_h = 56.94;  G_t = 1.0;   // glass outline W x H x thickness
A_w  = 139.00;  A_h = 47.74;               // active area (must stay uncovered)
// NOTE: the real panel's active area is offset toward the FPC edge — this model
// centres it. << MEASURE >> your panel's border and shift screen_off if needed.
screen_off = 0;                            // (legacy) kept 0; see active_off_*
// This panel's flex (FPC) leaves the LEFT short edge — the user's left as they
// face the screen, i.e. the low-X side (world x < W/2). The aperture is centred
// on the ACTIVE area, which sits off-centre on the glass — measure yours and
// nudge these (+x = toward the right, away from the FPC edge). << MEASURE >>
active_off_x = 0;
active_off_y = 0;

// ---- screen retention (glueless) ------------------------------------------
lip_over  = 4.0;   // how far the front bezel lip overlaps the glass border
lip_t     = 1.4;   // deck material left in FRONT of the glass (the visible lip)
glass_gap = 0.5;   // clearance around the glass in its pocket
foam_t    = 1.0;   // non-adhesive closed-cell foam gasket behind the glass
bracket_t = 2.6;   // printed retaining frame thickness
fpc_w     = 26;    // ribbon-slot span along the LEFT short edge (the FPC side)

// ---- deck nameplate (engraved, faces the user) ----------------------------
name_text  = "TYPOENA";
name_size  = 6.5;             // cap height in mm
name_depth = 0.8;             // engrave depth — raise for a bolder, deeper cut
name_font  = "Monaspace Krypton";   // install once — see README (Nameplate font)

A_ap_w = A_w + 2;                  // through-aperture (a hair bigger than active)
A_ap_h = A_h + 1;                  //   still smaller than glass minus 2*lip
P_w    = G_w + glass_gap;          // glass pocket (locates the glass in X/Y)
P_h    = G_h + glass_gap;

// screen placed centred on the deck (measured up the slope)
deck_L    = (D - 2*corner_r) / cos(theta);   // deck length along the slope
screen_cy = deck_L/2;                        // centre it
boss_r    = 3.4;                             // M2 self-tap boss for the bracket

// ---- ports on the back wall  (ESP32-S3-DevKitC-1 edge) --------------------
port_z   = 7;      // height of the port centres off the desk   << MEASURE >>
usbc_w   = 9.6;  usbc_h = 3.6;               // USB-C opening (with clearance)
sd_w     = 12;   sd_h   = 2.4;               // microSD slot
// X positions of the three openings along the back << MEASURE to your board >>
port_x   = [W/2 - 15, W/2, W/2 + 17];        // usb-c (kbd), usb-c (power), µSD

// ---- baseplate / chassis --------------------------------------------------
bp_t       = 2.6;  // baseplate thickness
bp_gap     = 0.5;  // clearance so it drops into the shell
foot_r     = 7;    // round feet (the little typewriter feet)
foot_h     = 3.5;
post_r     = 4.2;  // corner screw posts inside the shell (M2.5 self-tap)
post_pilot = 1.15;

// board mounting standoffs on the baseplate  << MEASURE hole positions >>
standoff_h     = 6;
standoff_pilot = 1.15;
// ESP32-S3-DevKitC-1 is ~70 x 28 mm; these are PLACEHOLDER hole coords:
esp_holes  = [[W/2-33, 30],[W/2+33, 30],[W/2-33, 54],[W/2+33, 54]];
// DESPI-C579 breakout sits in the cavity on the LEFT, under the FPC exit; SPI
// wires (MOSI/SCLK/CS/DC/RST/BUSY + 3V3/GND) run from here across to the ESP32.
// PLACEHOLDER hole coords << MEASURE >>:
brk_holes  = [[22, 40],[22, 66]];

// ---- colours (for the assembled render) -----------------------------------
C_body   = "#3c6382";
C_plate  = "#C9C3B2";
C_bracket= "#2B2B2B";
C_screen = "#F7F4EA";
C_foam   = "#8a8f94";

// ===========================================================================
//  helpers
// ===========================================================================
module rrect(w, d, r) {                       // 2D rounded rectangle, centred
    hull() for (mx=[-1,1], my=[-1,1])
        translate([mx*(w/2-r), my*(d/2-r)]) circle(r=r);
}

// place children onto the reclined deck plane. Origin at the FRONT-TOP edge
// (world y=0, z=Hf) — where the true hull top surface actually begins; anchor
// it at the pillar centre instead and everything lands ~3mm below the surface.
// local frame: X = width, Y = up the slope, Z = out of the deck (normal).
module on_deck() {
    translate([W/2, 0, Hf]) rotate([theta, 0, 0]) children();
}

// ===========================================================================
//  body
// ===========================================================================
module body_outer() {
    hull() {
        translate([corner_r,     corner_r,     0]) cylinder(h=Hf, r=corner_r);
        translate([W-corner_r,    corner_r,     0]) cylinder(h=Hf, r=corner_r);
        translate([corner_r,      D-corner_r,   0]) cylinder(h=Hb, r=corner_r);
        translate([W-corner_r,    D-corner_r,   0]) cylinder(h=Hb, r=corner_r);
    }
}

module body_cavity() {
    ri = corner_r - wall;
    hull() {
        translate([corner_r,   corner_r,   -3]) cylinder(h=Hf-top_wall+3, r=ri);
        translate([W-corner_r, corner_r,   -3]) cylinder(h=Hf-top_wall+3, r=ri);
        translate([corner_r,   D-corner_r, -3]) cylinder(h=Hb-top_wall+3, r=ri);
        translate([W-corner_r, D-corner_r, -3]) cylinder(h=Hb-top_wall+3, r=ri);
    }
}

// 4 corner posts the baseplate screws up into
module corner_posts() {
    for (px=[corner_r+3, W-corner_r-3], py=[corner_r+3, D-corner_r-3]) {
        h = (py < D/2) ? Hf-top_wall : Hb-top_wall;
        translate([px, py, 0]) difference() {
            cylinder(h=h, r=post_r);
            translate([0,0,-1]) cylinder(h=h+2, r=post_pilot);
        }
    }
}

// 4 bosses just OUTSIDE the glass pocket for the retaining bracket
module bracket_bosses() {
    on_deck() for (bx=[-(P_w/2+5), P_w/2+5],
                   by=[screen_cy-(P_h/2+5), screen_cy+(P_h/2+5)]) {
        blen = lip_t + G_t + foam_t + bracket_t + 6;
        translate([bx, by, -lip_t-blen]) difference() {
            cylinder(h=blen, r=boss_r);
            translate([0,0,-1]) cylinder(h=blen+2, r=1.0);   // M2 self-tap
        }
    }
}

// deck cuts: through-aperture, glass pocket (leaves the front lip), FPC slot
module screen_cuts() {
    on_deck() translate([0, screen_cy, 0]) {
        // window — centred on the ACTIVE area (offset toward the FPC/left edge)
        translate([active_off_x, active_off_y, -30])
            cube([A_ap_w, A_ap_h, 66], center=true);
        // glass pocket behind the lip — centred on the glass outline
        translate([0, 0, -30-lip_t]) cube([P_w, P_h, 60], center=true);
        // FPC clearance: an internal notch in the LEFT recess wall, kept BELOW
        // the bezel lip so it stays invisible from outside — the flex passes the
        // glass's left edge and folds back into the cavity, to the breakout
        translate([-P_w/2, 0, -30-lip_t]) cube([14, fpc_w, 60], center=true);
    }
}

module port_cuts() {
    // USB-C x2 + microSD through the back wall (y = D)
    for (i=[0:2]) {
        pw = (i==2) ? sd_w   : usbc_w;
        ph = (i==2) ? sd_h   : usbc_h;
        translate([port_x[i], D-wall-1, port_z])
            rotate([-90,0,0]) linear_extrude(wall+2)
                offset(r=0.8) square([pw-1.6, ph-1.6], center=true);
    }
}

// engraved nameplate on the DECK, in the band between the front edge and the
// screen — faces the user as they write. Sits flat on the reclined deck.
module nameplate() {
    name_y = (screen_cy - P_h/2) / 2;     // centre of the front deck band
    on_deck() translate([screen_off, name_y, -name_depth])
        linear_extrude(name_depth + 0.6)
            text(name_text, size=name_size, halign="center", valign="center",
                 font=name_font, spacing=1.1);
}

module case_body() {
    difference() {
        union() {
            difference() { body_outer(); body_cavity(); }
            corner_posts();
            bracket_bosses();
        }
        screen_cuts();
        port_cuts();
        nameplate();                 // engrave (comment out for a blank face)
    }
}

// ===========================================================================
//  screen retaining bracket  (printed flat, screwed to the 4 bosses)
// ===========================================================================
module bracket() {
    ow = P_w + 18; oh = P_h + 18;
    difference() {
        linear_extrude(bracket_t)
            difference() { rrect(ow, oh, 4); rrect(A_ap_w+2, A_ap_h+2, 2); }
        for (bx=[-(P_w/2+5), P_w/2+5], by=[-(P_h/2+5), P_h/2+5])
            translate([bx, by, -1]) cylinder(h=bracket_t+2, r=1.45);   // M2 clear
    }
}

// ===========================================================================
//  baseplate / chassis
// ===========================================================================
module baseplate() {
    iw = W - 2*wall - bp_gap;
    id = D - 2*wall - bp_gap;
    difference() {
        union() {
            // plate (centred on the footprint)
            translate([W/2, D/2, 0]) linear_extrude(bp_t) rrect(iw, id, corner_r-wall);
            // round feet underneath
            for (fx=[corner_r+6, W-corner_r-6], fy=[corner_r+6, D-corner_r-6])
                translate([fx, fy, -foot_h]) cylinder(h=foot_h+0.1, r=foot_r);
            // board standoffs on top
            for (h = concat(esp_holes, brk_holes))
                translate([h[0], h[1], bp_t]) cylinder(h=standoff_h, r=3);
        }
        // corner screw clearance (into the body posts)
        for (px=[corner_r+3, W-corner_r-3], py=[corner_r+3, D-corner_r-3])
            translate([px, py, -foot_h-1]) cylinder(h=bp_t+foot_h+2, r=1.6);
        // standoff pilot holes
        for (h = concat(esp_holes, brk_holes))
            translate([h[0], h[1], bp_t-1]) cylinder(h=standoff_h+2, r=standoff_pilot);
        // cable / connector relief at the back
        translate([W/2, D-wall-3, -1]) cube([30, 8, bp_t+2], center=false);
    }
}

// ===========================================================================
//  assemblies
// ===========================================================================
module ghost_screen() {
    on_deck() translate([screen_off, screen_cy+screen_off, -lip_t-G_t/2])
        color(C_screen) cube([G_w, G_h, G_t], center=true);
}
module placed_bracket() {
    on_deck() translate([screen_off, screen_cy+screen_off,
                         -lip_t-G_t-foam_t-bracket_t])
        color(C_bracket) bracket();
}
// foam gasket (non-adhesive) — a border frame between glass and bracket
module foam() {
    linear_extrude(foam_t)
        difference() { rrect(P_w+4, P_h+4, 3); rrect(A_ap_w, A_ap_h, 2); }
}
module placed_foam() {
    on_deck() translate([screen_off, screen_cy+screen_off, -lip_t-G_t-foam_t])
        color(C_foam) foam();
}

if (show == "assembled") {
    color(C_body)   case_body();
    ghost_screen();
    placed_bracket();
    translate([0,0,-0.01]) color(C_plate) baseplate();
} else if (show == "body") {
    color(C_body) case_body();
} else if (show == "bracket") {
    color(C_bracket) bracket();
} else if (show == "baseplate") {
    color(C_plate) baseplate();
} else if (show == "print_plate") {
    color(C_body)    case_body();
    translate([W+30, 0, 0])           color(C_plate)   baseplate();
    translate([W+30, D+30, foot_h])   color(C_bracket) bracket();
} else if (show == "section") {
    // slice away the +X half: the cut face shows the screen clamp, and the
    // retained LEFT half exposes the internal FPC clearance behind the bezel
    difference() {
        union() {
            color(C_body)   case_body();
            ghost_screen();
            placed_foam();
            placed_bracket();
            translate([0,0,-0.01]) color(C_plate) baseplate();
        }
        translate([W/2, -30, -70]) cube([W, D+60, 220]);
    }
}
