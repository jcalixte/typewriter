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
//    "section"     – vertical cross-section: how the screen is trapped
//    "plan"        – exploded horizontal section: deck lifted off the cavity
//    "plan_up"     – just the top half (deck / screen / bracket)
//    "plan_down"   – just the bottom half (cavity: standoffs, posts, ports)
// ============================================================================

show = "assembled";
$fn = 20;

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

// ---- mounting, boards & battery (defined here: the ports below depend on it)
bp_t           = 2.6;    // baseplate thickness
standoff_h     = 3;      // board standoff height — keep LOW so 22 mm PCB 1 clears the deck
standoff_pilot = 0.8;    // pilot Ø1.6 for an M2 self-tapper (PCB holes are Ø2)
pcb_t          = 1.6;    // PCB thickness (for port-height maths)
// PCB 1 = ESP32 devkit + e-ink driver + MT3608 boost. 70(X) x 50(Y), back-LEFT:
// short FPC to the deck's left slot, and the 22 mm Dupont/header side sits under
// the tall REAR of the wedge. Rigid board is only 10 mm; 22 mm is the vertical
// F-F jumpers. Its own USB-C is reached by opening the case — no wall cutout.
pcb1_x0 = 4;             pcb1_x1 = pcb1_x0 + 70;   // X  4 .. 74
pcb1_y0 = 46;            pcb1_y1 = pcb1_y0 + 50;   // Y 46 .. 96  (behind screen mid)
pcb1_h  = 22;            // tallest point (rigid stack + vertical Dupont)
// PCB 2 = µSD + 2x USB-C + TP4056. 80(X) x 20(Y), along the BACK wall, right end;
// connectors overhang its back edge by 8 mm to meet the wall.
pcb2_x1 = W - wall - 6;  pcb2_x0 = pcb2_x1 - 80;   // X ~87.6 .. 167.6
pcb2_y1 = D - wall - 8;  pcb2_y0 = pcb2_y1 - 20;   // back edge 8 mm off the wall
// corner holes, centres 2 mm in from each edge (Ø2 hole, 1 mm pad)
pcb1_holes = [[pcb1_x0+2,pcb1_y0+2],[pcb1_x1-2,pcb1_y0+2],
              [pcb1_x0+2,pcb1_y1-2],[pcb1_x1-2,pcb1_y1-2]];
pcb2_holes = [[pcb2_x0+2,pcb2_y0+2],[pcb2_x1-2,pcb2_y0+2],
              [pcb2_x0+2,pcb2_y1-2],[pcb2_x1-2,pcb2_y1-2]];
// LiPo 3700 mAh (96 x 33.5 x 10.3), flat across the FRONT — dead wedge space,
// CG low + forward. Leads exit toward the charger (back-right). << confirm cell >>
bat_w = 96;  bat_d = 33.5;  bat_h = 10.3;
bat_y0 = wall + 4;                             // front edge just off the front wall

// ---- ports on the back wall  (I/O board = PCB 2) --------------------------
// PCB 2 lies flat at the back-right; its connectors overhang the board's back
// edge by 8 mm and face out through the BACK wall (horizontal insertion). The
// µSD/reset end faces the case's RIGHT wall, so from the +X (right) end inward
// the order is: reset, µSD, keyboard, charge.
// Openings measured off the real parts:
usbc_w   = 8.0;  usbc_h = 2.5;               // USB-C shell opening (W x H)
sd_w     = 13.0; sd_h   = 2.0;               // microSD slot (W x H)
usbc_cz  = 3.5;                              // USB-C opening centre, above PCB top
sd_cz    = 2.5;                              // µSD slot centre — aligned with the USB-Cs, 1 mm lower
pcb2_z   = bp_t + standoff_h + pcb_t;        // PCB 2 top face height off the floor
// per-port centre heights off the floor       [charge, keyboard, µSD]
port_z   = [pcb2_z+usbc_cz, pcb2_z+usbc_cz, pcb2_z+sd_cz];
// PCB 2 is flipped vs how you view it: the charge end sits inward (low X), the
// µSD/reset end faces the RIGHT wall. Same 8/7/5 gaps, measured from the LEFT
// board edge (pcb2_x0):
//   charge : 8 gap + 8/2 -> 12.0   keyboard: +8+7+8/2 -> 27.0   µSD: +8+5+13/2 -> 42.5
port_x   = [pcb2_x0+12, pcb2_x0+27, pcb2_x0+42.5];   // charge, keyboard, µSD

// ---- reset button (momentary wired to EN/GND) -----------------------------
// Our OWN switch, soldered to the board's EN + GND header pins — NOT the
// DevKitC's on-board buttons (top-actuated and buried once the board lies flat
// on its standoffs). It sits on the back wall, out past the µSD, so it's never
// hit while typing. BOOT is left off on purpose: on the S3, auto-download
// (UART-bridge DTR/RTS or the native USB-Serial-JTAG) makes it recovery-only —
// not worth a fat-fingerable button on a writing appliance.
rst_btn  = true;             // set false to omit the reset hole entirely
rst_d    = 7.2;              // through-hole Ø for the switch barrel   << MEASURE >>
rst_x    = pcb2_x0 + 55;     // µSD side, toward the RIGHT wall (past the µSD @ +42.5)
rst_z    = pcb2_z + 4;       // a touch above the port row

// ---- baseplate / chassis --------------------------------------------------
bp_gap     = 0.5;  // clearance so it drops into the shell
foot_r     = 7;    // round feet (the little typewriter feet)
foot_h     = 3.5;
post_r     = 4.2;  // corner screw posts inside the shell (M2.5 self-tap)
post_pilot = 1.15;

// ---- colours (for the assembled render) -----------------------------------
C_body   = "#B6CEB4";
C_plate  = "#C9C3B2";
C_bracket= "#2B2B2B";
C_screen = "#F7F4EA";
C_foam   = "#8a8f94";

// ---- cutaway sections -----------------------------------------------------
plan_z       = 22;   // height of the horizontal "plan" cut
plan_explode = 62;   // gap between the halves in the exploded "plan" view

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
    // USB-C (charge, keyboard) + microSD through the BACK wall (y = D)
    for (i=[0:2]) {
        pw = (i==2) ? sd_w   : usbc_w;
        ph = (i==2) ? sd_h   : usbc_h;
        translate([port_x[i], D-wall-1, port_z[i]])
            rotate([-90,0,0]) linear_extrude(wall+2)
                offset(r=0.8) square([pw-1.6, ph-1.6], center=true);
    }
}

// reset switch mounting hole through the back wall (y = D)
module reset_cut() {
    if (rst_btn)
        translate([rst_x, D-wall-1, rst_z])
            rotate([-90,0,0]) cylinder(h=wall+2, r=rst_d/2);
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
        reset_cut();
        nameplate();                 // engrave (comment out for a blank face)
    }
}

// ===========================================================================
//  screen retaining bracket  (printed flat, screwed to the 4 bosses)
// ===========================================================================
module bracket() {
    ow = P_w + 18; oh = P_h + 18;
    // FPC U-turn clearance: a gap in the LEFT frame member. The flex leaves the
    // glass's back plane and folds ~180° to dive into the cavity toward the
    // breakout; a safe bend radius (~1.5-2 mm) makes that loop ~4 mm deep, too
    // deep for the 1 mm foam gap, so it fouls this rigid frame unless relieved
    // here. Lines up with the body's FPC slot (screen_cuts) and the foam relief.
    difference() {
        linear_extrude(bracket_t)
            difference() {
                rrect(ow, oh, 4);
                rrect(A_ap_w+2, A_ap_h+2, 2);
                translate([-(ow + A_ap_w+2)/4, 0])
                    square([(ow - (A_ap_w+2))/2 + 4, fpc_w], center=true);
            }
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
            // board standoffs on top (PCB 1 back-left + PCB 2 back-right)
            for (h = concat(pcb1_holes, pcb2_holes))
                translate([h[0], h[1], bp_t]) cylinder(h=standoff_h, r=3);
            // battery cage nibs (front LiPo; foam/VHB tape does the rest)
            for (cx=[W/2-bat_w/2-1, W/2+bat_w/2+1], cy=[bat_y0-1, bat_y0+bat_d+1])
                translate([cx, cy, bp_t]) cylinder(h=5, r=1.6);
        }
        // corner screw clearance (into the body posts)
        for (px=[corner_r+3, W-corner_r-3], py=[corner_r+3, D-corner_r-3])
            translate([px, py, -foot_h-1]) cylinder(h=bp_t+foot_h+2, r=1.6);
        // standoff pilot holes
        for (h = concat(pcb1_holes, pcb2_holes))
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
// LiPo lying flat on the baseplate at the front
module ghost_battery() {
    translate([W/2, bat_y0+bat_d/2, bp_t+bat_h/2])
        color("#3f7d4f") cube([bat_w, bat_d, bat_h], center=true);
}
// a board slab on its standoffs + a translucent envelope for its tall parts
module ghost_pcb(x0, y0, x1, y1, htot) {
    w = x1-x0; d = y1-y0;
    translate([(x0+x1)/2, (y0+y1)/2, bp_t+standoff_h]) color("#2f6f4f") {
        linear_extrude(pcb_t) square([w, d], center=true);
        translate([0,0,pcb_t]) %linear_extrude(htot-pcb_t)
            square([w*0.7, d*0.7], center=true);
    }
}
module ghost_boards() {
    ghost_battery();
    ghost_pcb(pcb1_x0, pcb1_y0, pcb1_x1, pcb1_y1, pcb1_h);   // back-left, tall
    ghost_pcb(pcb2_x0, pcb2_y0, pcb2_x1, pcb2_y1, 8);        // back-right, low I/O
}
module placed_bracket() {
    on_deck() translate([screen_off, screen_cy+screen_off,
                         -lip_t-G_t-foam_t-bracket_t])
        color(C_bracket) bracket();
}
// foam gasket (non-adhesive) — a border frame between glass and bracket, with
// its LEFT border opened over the FPC span so the U-turning flex isn't clamped
module foam() {
    linear_extrude(foam_t)
        difference() {
            rrect(P_w+4, P_h+4, 3);
            rrect(A_ap_w, A_ap_h, 2);
            translate([-((P_w+4) + A_ap_w)/4, 0])
                square([((P_w+4) - A_ap_w)/2 + 4, fpc_w], center=true);
        }
}
module placed_foam() {
    on_deck() translate([screen_off, screen_cy+screen_off, -lip_t-G_t-foam_t])
        color(C_foam) foam();
}
// full coloured assembly, reused by the plan sections
module plan_assembly() {
    color(C_body)   case_body();
    ghost_screen();
    placed_foam();
    placed_bracket();
    ghost_boards();
    translate([0,0,-0.01]) color(C_plate) baseplate();
}
// the two halves of the horizontal cut at plan_z
module plan_down() {     // bottom: the cavity (standoffs, posts, ports)
    intersection() {
        plan_assembly();
        translate([-60, -60, plan_z-200]) cube([W+120, D+120, 200]);
    }
}
module plan_up() {       // top: the deck / lid (screen, bracket)
    intersection() {
        plan_assembly();
        translate([-60, -60, plan_z]) cube([W+120, D+120, 200]);
    }
}

if (show == "assembled") {
    color(C_body)   case_body();
    ghost_screen();
    placed_bracket();
    ghost_boards();
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
    // VERTICAL slice (remove +X half): cut face shows the screen clamp, and the
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
} else if (show == "plan") {
    // EXPLODED horizontal section: deck/lid half lifted off the cavity half
    plan_down();
    translate([0, 0, plan_explode]) plan_up();
} else if (show == "plan_up") {
    plan_up();       // just the top half — deck, screen, bracket
} else if (show == "plan_down") {
    plan_down();     // just the bottom half — cavity, standoffs, ports
}