# Quality Function Deployment

Translates what the device must _be_ (user-facing requirements) into what it
must _achieve_ (engineering characteristics) and what we must _build_
(components).
Surfaces the few targets that dominate the design and the conflicts between
them. Every decision cell points back to [`adr.md`](adr.md).

Scope: the shipped device — v0.1 delivered 2026-07-11, v0.5–v0.7 delivered
2026-07-12/14, v0.9 onboarding in flight (see
[`v0.1-mvp-product.md`](v0.1-mvp-product.md),
[`v0.5-palette-and-multi-file.md`](v0.5-palette-and-multi-file.md),
[`v0.7-search-and-git.md`](v0.7-search-and-git.md),
[`v0.9-onboarding-wizard.md`](v0.9-onboarding-wizard.md)) — **plus the
companion products** that now deliver the getting-started outcome: the macOS
installer ([`../installer/DESIGN.md`](../installer/DESIGN.md)), the
typoena.dev site with its `install.sh` one-liner, and the Typoena GitHub App
(device-flow auth shared by installer and on-device wizard). The remaining
v0.8–v1.0 trajectory ([README](../README.md), [macroplan](macroplan.md)) is
kept in mind so we don't paint into a corner. Terminology
(e.g. **Tracked**, **Local**, **Save**, **Publish**) follows the project
glossary at [`../CONTEXT.md`](../CONTEXT.md).

Format inspired by the classic House of Quality, kept compact. Strength
weights: **9** strong, **3** medium, **1** weak, blank none. This one file
owns everything: the House diagram itself (matrix, roof, basement Σ, and the
guessed competitor perception zone), hoisted to the top (just below); the
WHAT/HOW catalogues (§1, §2); the narrative reading of the numbers (§3, §4);
and the downstream sections (§5–§8). (The House was a separate `quality-house.md`
until 2026-07-11, merged into §3 to end the mirror-drift between the two files;
the diagram was lifted above §1 on 2026-07-11 so the picture leads.)

---

## House of Quality — the diagram

The artifact this whole document builds and reads is shown first: §1's WHATs (rows) × §2's HOWs (columns), scored 9 / 3 / 1 / blank, with the roof correlations (§4), the basement Σ / relative weights, and the right-hand competitive-perception zone. §1–§8 define, prioritise, and read it; the sync rules and a blank practice copy travel with the diagram just below.

> **Single source of truth.** The `\foreach` blocks in the diagram restate §1's
> weights and §2's targets — TikZ can't read the tables, so keep them in sync when
> either changes, and **recompute the basement Σ / Rel % here** (see
> [Regenerating](#regenerating)) rather than transcribing them from elsewhere.
> This mirror used to live in a separate `quality-house.md`; it was merged into §3
> on 2026-07-11 so the two can no longer silently drift.

For a blank version of the same chassis (WHATs, HOWs, importance, and v0.1
targets kept; relations + roof + Σ basement left empty for practice), see
[`quality-house-empty.md`](quality-house-empty.md).

```tikz
% =====================================================================
% QFD "House of Quality" preamble
% =====================================================================
\usetikzlibrary{arrows.meta, positioning, shapes.geometric, shapes.misc, calc, fit, backgrounds}

\newif\ifqfdshowroof          \qfdshowrooftrue
\newif\ifqfdshowbasement      \qfdshowbasementtrue
\newif\ifqfdshowcompetitive   \qfdshowcompetitivetrue
\newif\ifqfdshowlegend        \qfdshowlegendtrue
\newif\ifqfdshowimportance    \qfdshowimportancetrue
\newif\ifqfdshowcorrlegend    \qfdshowcorrlegendtrue
\newif\ifqfdshowevallegend    \qfdshowevallegendtrue

\def\qfdNW{5}
\def\qfdNH{5}
\def\qfdWhatW{4.0}
\def\qfdImpW{0.9}
\def\qfdCmpW{3}
\def\qfdHdrH{2.6}
\def\qfdBasementN{4}

\def\qfdWhatsTitle{Customer needs}
\def\qfdImpTitle{Imp.\ \%}
\def\qfdPerceptionTitle{Comparative evaluation}
\def\qfdPoorLabel{poor}
\def\qfdExcellentLabel{excellent}
\def\qfdAltOneLabel{Typoena}
\def\qfdAltTwoLabel{Competitor A}
\def\qfdAltThreeLabel{Competitor B}
\def\qfdRelTitle{Relation}
\def\qfdCorrTitle{Correlation}
\def\qfdEvalTitle{Evaluation}

\tikzset{
  qfdthin/.style ={line width=0.35pt},
  qfdmed/.style  ={line width=0.7pt},
  qfdstrong/.style={circle, draw, fill=black,
                    minimum size=7pt, inner sep=0pt},
  qfdmod/.style  ={circle, draw,
                    minimum size=7pt, inner sep=0pt, line width=0.8pt},
  qfdweak/.style ={regular polygon, regular polygon sides=3, draw,
                    minimum size=8.5pt, inner sep=0pt, line width=0.7pt},
  qfdrel/.is choice,
  qfdrel/S/.style={qfdstrong},
  qfdrel/M/.style={qfdmod},
  qfdrel/W/.style={qfdweak},
  qfdalt1mk/.style={circle, draw, fill=black,
                    minimum size=6pt, inner sep=0pt, line width=1pt},
  qfdalt1ln/.style={line width=1.2pt},
  qfdalt2mk/.style={regular polygon, regular polygon sides=3, draw,
                    fill=black, minimum size=6pt, inner sep=0pt,
                    line width=0.7pt},
  qfdalt2ln/.style={line width=0.7pt, dashed},
  qfdalt3mk/.style={rectangle, draw, fill=black,
                    minimum size=5pt, inner sep=0pt, line width=0.7pt},
  qfdalt3ln/.style={line width=0.7pt, dotted},
}

\newcommand{\qfdDrawGrid}{%
  \foreach \c in {1,...,\qfdNHm} \draw[qfdthin] (\c, 0) -- (\c, -\qfdNW);
  \foreach \r in {1,...,\qfdNWm} \draw[qfdthin] (0, -\r) -- (\qfdNH, -\r);
  \foreach \r in {1,...,\qfdNWm}
    \draw[qfdthin] (\qfdLeftEdge, -\r) -- (0, -\r);
  \ifqfdshowroof
    \foreach \c in {1,...,\qfdNHm}
      \draw[qfdthin] (\c, 0) -- (\c, \qfdHdrH);
  \fi
  \ifqfdshowcompetitive
    \foreach \r in {1,...,\qfdNWm}
      \draw[qfdthin] (\qfdNH, -\r) -- (\qfdNH+\qfdCmpW, -\r);
  \fi
  \ifqfdshowbasement
    \foreach \r in {1,...,\qfdBasementN}
      \draw[qfdthin] (0, -\qfdNW-\r) -- (\qfdNH, -\qfdNW-\r);
    \foreach \c in {1,...,\qfdNHm}
      \draw[qfdthin] (\c, -\qfdNW) -- (\c, -\qfdNW-\qfdBasementN);
  \fi
}

\newcommand{\qfdDrawRoof}{%
  \ifqfdshowroof
    \foreach \k in {1,...,\qfdNHm} {%
      \pgfmathsetmacro{\rx}{(\k+\qfdNH)/2}
      \pgfmathsetmacro{\ry}{\qfdHdrH + (\qfdNH-\k)/2}
      \pgfmathsetmacro{\lx}{\k/2}
      \pgfmathsetmacro{\ly}{\qfdHdrH + \k/2}
      \draw[qfdthin] (\k, \qfdHdrH) -- (\rx, \ry);
      \draw[qfdthin] (\k, \qfdHdrH) -- (\lx, \ly);
    }%
    \draw[qfdmed] (0, \qfdHdrH)
       -- (\qfdNH/2, \qfdApexY) -- (\qfdNH, \qfdHdrH);
    \foreach \i in {1,...,\qfdNH}
      \foreach \k in {1,...,\qfdNH} {%
        \pgfmathtruncatemacro{\jj}{\i+\k}
        \ifnum\jj>\qfdNH\relax\else
          \pgfmathsetmacro{\xx}{\i + \k/2 - 0.5}
          \pgfmathsetmacro{\yy}{\qfdHdrH + \k/2}
          \coordinate (C-\i-\jj) at (\xx, \yy);
        \fi
      }%
  \fi
}

\newcommand{\qfdDrawScale}{%
  \ifqfdshowcompetitive
    \foreach \tk in {0,1,2,3,4,5} {%
      \pgfmathsetmacro{\tx}{\qfdNH + (\tk+0.5)*\qfdCmpW/6}
      \node[anchor=south, font=\scriptsize] at (\tx, 0.02) {\tk};
    }%
    \node[anchor=south, font=\scriptsize\bfseries, align=center,
          text width=\qfdCmpW cm]
         at ({\qfdNH + \qfdCmpW/2}, 0.7) {\qfdPerceptionTitle};
    \node[anchor=north, font=\scriptsize\itshape]
         at ({\qfdNH + 0.45}, -\qfdNW) {\qfdPoorLabel};
    \node[anchor=north, font=\scriptsize\itshape]
         at ({\qfdNH + \qfdCmpW - 0.45}, -\qfdNW) {\qfdExcellentLabel};
  \fi
}

\newcommand{\qfdDrawZoneTitles}{%
  \ifqfdshowimportance
    \node[rotate=90, anchor=west, font=\footnotesize\bfseries]
         at ({-\qfdImpW/2}, 0.12) {\qfdImpTitle};
  \fi
  \node[font=\scriptsize\bfseries, align=center, text width=\qfdWhatW cm]
       at ({\qfdLeftEdge + \qfdWhatW/2},
           {\ifqfdshowroof \qfdHdrH/2 \else 0.6 \fi}) {\qfdWhatsTitle};
}

\newcommand{\qfdDrawFrames}{%
  \begin{scope}[qfdmed]
    \draw (\qfdLeftEdge, 0) rectangle (\qfdNH, -\qfdNW);
    \ifqfdshowimportance \draw (-\qfdImpW, 0) -- (-\qfdImpW, -\qfdNW); \fi
    \draw (0, 0) -- (0, -\qfdNW);
    \ifqfdshowroof
      \draw (0, 0) rectangle (\qfdNH, \qfdHdrH); \fi
    \ifqfdshowbasement
      \draw (0, -\qfdNW) rectangle (\qfdNH, -\qfdNW-\qfdBasementN); \fi
    \ifqfdshowcompetitive
      \draw (\qfdNH, 0) rectangle (\qfdNH+\qfdCmpW, -\qfdNW); \fi
  \end{scope}
}

\newcommand{\qfdDrawLegend}{%
  \ifqfdshowlegend
    \pgfmathsetmacro{\qfdLegX}{%
      \qfdNH + \ifqfdshowcompetitive \qfdCmpW + 0.7 \else 0.7 \fi}
    \pgfmathsetmacro{\qfdLegBottom}{%
      -2.05
      \ifqfdshowroof    \ifqfdshowcorrlegend - 2.55 \fi \fi
      \ifqfdshowcompetitive \ifqfdshowevallegend - 2.20 \fi \fi}
    \pgfmathsetmacro{\qfdLegY}{\qfdHdrH - 0.4}
    \begin{scope}[shift={(\qfdLegX, \qfdLegY)}]
      \draw[qfdmed, rounded corners=2pt]
        (-0.15, 0.4) rectangle (4.5, \qfdLegBottom);
      \node[anchor=west, font=\footnotesize\bfseries] at (0, 0.1)
        {\qfdRelTitle};
      \draw[qfdthin] (0, -0.15) -- (4.35, -0.15);
      \node[qfdstrong] at (0.22, -0.5)  {};
        \node[anchor=west] at (0.5, -0.5)  {Strong (9)};
      \node[qfdmod]    at (0.22, -0.95) {};
        \node[anchor=west] at (0.5, -0.95) {Medium (3)};
      \node[qfdweak]   at (0.22, -1.4)  {};
        \node[anchor=west] at (0.5, -1.4)  {Weak (1)};
      \ifqfdshowroof \ifqfdshowcorrlegend
        \node[anchor=west, font=\footnotesize\bfseries] at (0, -2.10)
          {\qfdCorrTitle};
        \draw[qfdthin] (0, -2.35) -- (4.35, -2.35);
        \node[anchor=west] at (0, -2.70) {{$+\!+$}\quad very positive};
        \node[anchor=west] at (0, -3.05) {{$+$\phantom{$+$}}\quad positive};
        \node[anchor=west] at (0, -3.40) {{$-$\phantom{$-$}}\quad negative};
        \node[anchor=west] at (0, -3.75) {{$-\!-$}\quad very negative};
      \fi \fi
      \ifqfdshowcompetitive \ifqfdshowevallegend
        \pgfmathsetmacro{\qfdEvalTop}{%
          -2.10 \ifqfdshowroof\ifqfdshowcorrlegend - 2.55 \fi\fi}
        \node[anchor=west, font=\footnotesize\bfseries]
          at (0, \qfdEvalTop) {\qfdEvalTitle};
        \pgfmathsetmacro{\qfdEvalSep}{\qfdEvalTop - 0.25}
        \draw[qfdthin] (0, \qfdEvalSep) -- (4.35, \qfdEvalSep);
        \pgfmathsetmacro{\qfdLegA}{\qfdEvalTop - 0.55}
        \draw[qfdalt1ln] (0.05, \qfdLegA) -- (0.45, \qfdLegA);
          \node[qfdalt1mk] at (0.25, \qfdLegA) {};
          \node[anchor=west, font=\bfseries] at (0.55, \qfdLegA)
            {\qfdAltOneLabel};
        \pgfmathsetmacro{\qfdLegB}{\qfdEvalTop - 0.95}
        \draw[qfdalt2ln] (0.05, \qfdLegB) -- (0.45, \qfdLegB);
          \node[qfdalt2mk] at (0.25, \qfdLegB) {};
          \node[anchor=west] at (0.55, \qfdLegB) {\qfdAltTwoLabel};
        \pgfmathsetmacro{\qfdLegC}{\qfdEvalTop - 1.35}
        \draw[qfdalt3ln] (0.05, \qfdLegC) -- (0.45, \qfdLegC);
          \node[qfdalt3mk] at (0.25, \qfdLegC) {};
          \node[anchor=west] at (0.55, \qfdLegC) {\qfdAltThreeLabel};
      \fi \fi
    \end{scope}
  \fi
}

\newenvironment{qfdhouse}{%
  \begin{tikzpicture}[x=1cm, y=1cm, font=\scriptsize,
                      line cap=round, line join=round]
  \ifqfdshowimportance
    \pgfmathsetmacro{\qfdLeftEdge}{-\qfdWhatW-\qfdImpW}
  \else
    \pgfmathsetmacro{\qfdLeftEdge}{-\qfdWhatW}
  \fi
  \pgfmathsetmacro{\qfdApexY}{\qfdHdrH + \qfdNH/2}
  \pgfmathtruncatemacro{\qfdNHm}{\qfdNH - 1}
  \pgfmathtruncatemacro{\qfdNWm}{\qfdNW - 1}
  \qfdDrawGrid
  \qfdDrawRoof
  \qfdDrawScale
  \qfdDrawZoneTitles
}{%
  \qfdDrawFrames
  \qfdDrawLegend
  \end{tikzpicture}%
}

% --- Dimensions tuned for the typewriter QFD (15 W x 15 H) ---
\def\qfdNW{15}
\def\qfdNH{15}
\def\qfdWhatW{4.6}
\def\qfdImpW{0.7}
\def\qfdHdrH{5.0}
\def\qfdBasementN{3}
\def\qfdCmpW{3.4}
\qfdshowlegendfalse                 % we draw a 4-alternative legend manually

\def\qfdWhatsTitle{User-facing requirements (W)}
\def\qfdImpTitle{Weight}
\def\qfdPerceptionTitle{Competitive perception\\(0–5, guessed)}
\def\qfdPoorLabel{poor}
\def\qfdExcellentLabel{excellent}

% Perception-zone markers: shape + colour-blind-safe colour per product.
% Palette is Okabe-Ito (blue, vermillion, bluish green, reddish purple).
% Light fills + saturated outlines keep stacked markers legible.
\definecolor{qfdcTypoena}{RGB}{0,114,178}
\definecolor{qfdcRem}{RGB}{213,94,0}
\definecolor{qfdcFrw}{RGB}{0,158,115}
\definecolor{qfdcPom}{RGB}{204,121,167}
\definecolor{qfdcFrwS}{RGB}{86,180,233}

\tikzset{
  qfdalt1mk/.style={circle, draw=qfdcTypoena, fill=qfdcTypoena!55!white,
                    minimum size=6.5pt, inner sep=0pt, line width=1.1pt},
  qfdalt1ln/.style={line width=1.2pt, qfdcTypoena},
  qfdalt2mk/.style={regular polygon, regular polygon sides=3,
                    draw=qfdcRem, fill=qfdcRem!55!white,
                    minimum size=7pt, inner sep=0pt, line width=0.9pt},
  qfdalt2ln/.style={line width=0.8pt, dashed, qfdcRem},
  qfdalt3mk/.style={rectangle, draw=qfdcFrw, fill=qfdcFrw!55!white,
                    minimum size=5.5pt, inner sep=0pt, line width=0.9pt},
  qfdalt3ln/.style={line width=0.8pt, dotted, qfdcFrw},
  qfdalt4mk/.style={diamond, aspect=1, draw=qfdcPom,
                    fill=qfdcPom!50!white,
                    minimum size=7pt, inner sep=0pt, line width=1.0pt},
  qfdalt4ln/.style={line width=0.8pt, dash dot, qfdcPom},
  qfdalt5mk/.style={regular polygon, regular polygon sides=5,
                    draw=qfdcFrwS, fill=qfdcFrwS!40!white,
                    minimum size=5.5pt, inner sep=0pt, line width=0.8pt},
  qfdalt5ln/.style={line width=0.7pt, dash dot dot, qfdcFrwS},
}

\begin{document}
\begin{qfdhouse}

  % ---------- WHATs (left column) ----------
  % Box width is 0.2 cm narrower than the column so labels have 0.1 cm
  % clearance on each side and don't bleed into the Weight column.
  \pgfmathsetmacro{\qfdWhatTextW}{\qfdWhatW - 0.2}
  \foreach \r/\t in {%
    1/{W1 Sub-second visible response to typing},
    2/{W2 Publishing is one deliberate action away},
    3/{W3 Pulling power never corrupts the file},
    4/{W4 Provisioning never interrupts a writing session},
    5/{W5 Quick boot to a writing cursor},
    6/{W6 Long sessions without crash, lag, drift},
    7/{W7 Nothing on the device competes with prose},
    8/{W8 The UI never moves except when I move it},
    9/{W9 Codebase absorbs the planned roadmap},
    10/{W10 I can repair or fork it with hobbyist tools},
    11/{W11 Multi-day battery life (v0.8 onward)},
    12/{W12 Local-only files coexist with git scope (v0.5+)},
    13/{W13 Typography sets a writing-tool tone},
    14/{W14 I can carry the device and write away from a desk},
    15/{W15 A first-time user reaches writing without developer tools}%
  }
    \node[anchor=west, font=\scriptsize,
          text width=\qfdWhatTextW cm, align=left]
      at ({\qfdLeftEdge + 0.1}, {-\r + 0.5}) {\t};

  % ---------- Importance (raw 1-10 weight) ----------
  \foreach \r/\w in {1/10, 2/9, 3/10, 4/7, 5/6, 6/9, 7/8, 8/7,
                     9/8, 10/5, 11/4, 12/5, 13/7, 14/8, 15/7}
    \node[font=\scriptsize] at ({-\qfdImpW/2}, {-\r + 0.5}) {\w};

  % ---------- HOWs (rotated column titles) ----------
  \foreach \c/\t in {%
    1/{H1 Type latency},
    2/{H2 Refresh area per keystroke},
    3/{H3 Full-refresh cadence},
    4/{H4 Boot latency (cold)},
    5/{H5 Continuous-typing endurance},
    6/{H6 Publish reliability},
    7/{H7 Publish latency},
    8/{H8 Save durability},
    9/{H9 Heap headroom (Publish)},
    10/{H10 Firmware binary size},
    11/{H11 Total stack budget},
    12/{H12 Network reconnect time},
    13/{H13 Idle / typing / push current},
    14/{H15 Clean release build time},
    15/{H16 Onboarding duration}%
  }
    \node[rotate=90, anchor=west, font=\scriptsize]
      at ({\c - 0.5}, 0.15) {\t};

  % ---------- Relation matrix (S=9, M=3, W=1) ----------
  % W1 row 1: H1S H2S H3M H5M H9W H11W
  \node[qfdrel/S] at ({1 - 0.5},  {-1 + 0.5}) {};
  \node[qfdrel/S] at ({2 - 0.5},  {-1 + 0.5}) {};
  \node[qfdrel/M] at ({3 - 0.5},  {-1 + 0.5}) {};
  \node[qfdrel/M] at ({5 - 0.5},  {-1 + 0.5}) {};
  \node[qfdrel/W] at ({9 - 0.5},  {-1 + 0.5}) {};
  \node[qfdrel/W] at ({11 - 0.5}, {-1 + 0.5}) {};

  % W2 row 2: H6S H7M H9S H12S
  \node[qfdrel/S] at ({6 - 0.5},  {-2 + 0.5}) {};
  \node[qfdrel/M] at ({7 - 0.5},  {-2 + 0.5}) {};
  \node[qfdrel/S] at ({9 - 0.5},  {-2 + 0.5}) {};
  \node[qfdrel/S] at ({12 - 0.5}, {-2 + 0.5}) {};

  % W3 row 3: H8S
  \node[qfdrel/S] at ({8 - 0.5},  {-3 + 0.5}) {};

  % W4 row 4: H6M H12M
  \node[qfdrel/M] at ({6 - 0.5},  {-4 + 0.5}) {};
  \node[qfdrel/M] at ({12 - 0.5}, {-4 + 0.5}) {};

  % W5 row 5: H4S H10M
  \node[qfdrel/S] at ({4 - 0.5},  {-5 + 0.5}) {};
  \node[qfdrel/M] at ({10 - 0.5}, {-5 + 0.5}) {};

  % W6 row 6: H1M H3M H5S H6M H8M H9S H11M H12M
  \node[qfdrel/M] at ({1 - 0.5},  {-6 + 0.5}) {};
  \node[qfdrel/M] at ({3 - 0.5},  {-6 + 0.5}) {};
  \node[qfdrel/S] at ({5 - 0.5},  {-6 + 0.5}) {};
  \node[qfdrel/M] at ({6 - 0.5},  {-6 + 0.5}) {};
  \node[qfdrel/M] at ({8 - 0.5},  {-6 + 0.5}) {};
  \node[qfdrel/S] at ({9 - 0.5},  {-6 + 0.5}) {};
  \node[qfdrel/M] at ({11 - 0.5}, {-6 + 0.5}) {};
  \node[qfdrel/M] at ({12 - 0.5}, {-6 + 0.5}) {};

  % W7 row 7: H1M H2M H3M H13M
  \node[qfdrel/M] at ({1 - 0.5},  {-7 + 0.5}) {};
  \node[qfdrel/M] at ({2 - 0.5},  {-7 + 0.5}) {};
  \node[qfdrel/M] at ({3 - 0.5},  {-7 + 0.5}) {};
  \node[qfdrel/M] at ({13 - 0.5}, {-7 + 0.5}) {};

  % W8 row 8: H1W H2S H3S
  \node[qfdrel/W] at ({1 - 0.5},  {-8 + 0.5}) {};
  \node[qfdrel/S] at ({2 - 0.5},  {-8 + 0.5}) {};
  \node[qfdrel/S] at ({3 - 0.5},  {-8 + 0.5}) {};

  % W9 row 9: H10W H11W H15M
  \node[qfdrel/W] at ({10 - 0.5}, {-9 + 0.5}) {};
  \node[qfdrel/W] at ({11 - 0.5}, {-9 + 0.5}) {};
  \node[qfdrel/M] at ({14 - 0.5}, {-9 + 0.5}) {};

  % W10 row 10: H10M H13W H15W
  \node[qfdrel/M] at ({10 - 0.5}, {-10 + 0.5}) {};
  \node[qfdrel/W] at ({13 - 0.5}, {-10 + 0.5}) {};
  \node[qfdrel/W] at ({14 - 0.5}, {-10 + 0.5}) {};

  % W11 row 11: H13S
  \node[qfdrel/S] at ({13 - 0.5}, {-11 + 0.5}) {};

  % W12 row 12: H6W H8M
  \node[qfdrel/W] at ({6 - 0.5},  {-12 + 0.5}) {};
  \node[qfdrel/M] at ({8 - 0.5},  {-12 + 0.5}) {};

  % W13 row 13: H9M
  \node[qfdrel/M] at ({9 - 0.5},  {-13 + 0.5}) {};

  % W14 row 14: H4W H8M H12M H13S
  \node[qfdrel/W] at ({4 - 0.5},  {-14 + 0.5}) {};
  \node[qfdrel/M] at ({8 - 0.5},  {-14 + 0.5}) {};
  \node[qfdrel/M] at ({12 - 0.5}, {-14 + 0.5}) {};
  \node[qfdrel/S] at ({13 - 0.5}, {-14 + 0.5}) {};

  % W15 row 15: H12W H16S
  \node[qfdrel/W] at ({12 - 0.5}, {-15 + 0.5}) {};
  \node[qfdrel/S] at ({15 - 0.5}, {-15 + 0.5}) {};

  % ---------- Roof correlations ----------
  \node[font=\scriptsize] at (C-1-2)   {$+\!+$};   % H1-H2 strong reinforce
  \node[font=\scriptsize] at (C-1-3)   {$-$};      % H1-H3 mild conflict
  \node[font=\scriptsize] at (C-1-5)   {$+$};      % H1-H5 mild reinforce
  \node[font=\scriptsize] at (C-1-13)  {$-$};      % H1-H13 mild conflict
  \node[font=\scriptsize] at (C-2-3)   {$+\!+$};   % H2-H3 strong reinforce
  \node[font=\scriptsize] at (C-2-13)  {$+$};      % H2-H13
  \node[font=\scriptsize] at (C-3-13)  {$+$};      % H3-H13
  \node[font=\scriptsize] at (C-4-10)  {$-$};      % H4-H10 boot vs binary
  \node[font=\scriptsize] at (C-5-6)   {$+$};      % H5-H6
  \node[font=\scriptsize] at (C-5-8)   {$+$};      % H5-H8
  \node[font=\scriptsize] at (C-5-9)   {$-\!-$};   % H5-H9 soak vs heap
  \node[font=\scriptsize] at (C-6-7)   {$+$};      % H6-H7
  \node[font=\scriptsize] at (C-6-9)   {$-\!-$};   % H6-H9 push vs heap
  \node[font=\scriptsize] at (C-6-12)  {$+\!+$};   % H6-H12
  \node[font=\scriptsize] at (C-7-9)   {$-$};      % H7-H9
  \node[font=\scriptsize] at (C-7-12)  {$+\!+$};   % H7-H12
  \node[font=\scriptsize] at (C-9-10)  {$-\!-$};   % H9-H10 heap vs binary
  \node[font=\scriptsize] at (C-10-14) {$-\!-$};   % H10-H15 binary vs build
  \node[font=\scriptsize] at (C-11-13) {$-$};      % H11-H13
  \node[font=\scriptsize] at (C-12-15) {$+$};      % H12-H16 reconnect helps clone

  % ---------- Basement: target / abs weight / rel weight % ----------
  \foreach \c/\tgt/\abs/\rel in {%
    1/{$\leq$400\,ms}/148/9,
    2/{$\leq$1 line}/177/11,
    3/{1 : 64}/144/9,
    4/{$\leq$5\,s}/62/4,
    5/{$\geq$1\,h}/111/7,
    6/{$\geq$95\,\%}/134/8,
    7/{$\leq$30\,s}/27/2,
    8/{100\,\%}/156/10,
    9/{$\geq$1\,MB}/193/12,
    10/{$\leq$2\,MB}/41/3,
    11/{$\leq$128\,KB}/45/3,
    12/{$\leq$30\,s}/160/10,
    13/{obs.}/137/8,
    14/{$\leq$7\,min}/29/2,
    15/{$\leq$10\,min}/63/4%
  } {
    \node[font=\scriptsize] at ({\c - 0.5}, {-\qfdNW - 0.5}) {\tgt};
    \node[font=\scriptsize] at ({\c - 0.5}, {-\qfdNW - 1.5}) {\abs};
    \node[font=\scriptsize\bfseries]
      at ({\c - 0.5}, {-\qfdNW - 2.5}) {\rel};
  }

  % ---------- Basement row labels (in the margin below WHATs) ----------
  \foreach \k/\lbl in {1/{Target (v0.1)}, 2/{$\Sigma$ abs}, 3/{Rel.\ \%}}
    \node[anchor=east, font=\scriptsize\itshape]
      at ({-0.1}, {-\qfdNW - \k + 0.5}) {\lbl};

  % ---------- Perception zone: 5 products x 15 WHATs (0-5 scores) ----------
  % Columns: \so=Typoena shipped (measured through 2026-07-16), \st=reMarkable 2 + Type Folio,
  %          \sf=Freewrite Traveler, \sg=Pomera DM250,
  %          \sh=Freewrite Smart Typewriter.
  % Pass 1: stash each score as a named coordinate so the profile lines
  % below can reuse it without recomputing.
  \foreach \r/\so/\st/\sf/\sg/\sh in {%
    1/3/1/4/5/3,
    2/5/4/4/2/4,
    3/4/4/2/2/2,
    4/5/2/2/5/2,
    5/4/3/4/5/4,
    6/4/3/4/5/4,
    7/5/2/5/5/5,
    8/4/3/4/5/4,
    9/4/3/2/1/2,
    10/5/4/2/1/2,
    11/1/5/5/4/5,
    12/4/1/2/3/2,
    13/3/5/2/2/2,
    14/2/4/5/5/1,
    15/4/2/3/5/3%
  } {
    \pgfmathsetmacro{\xo}{\qfdNH + (\so + 0.5)*\qfdCmpW/6}
    \pgfmathsetmacro{\xt}{\qfdNH + (\st + 0.5)*\qfdCmpW/6}
    \pgfmathsetmacro{\xf}{\qfdNH + (\sf + 0.5)*\qfdCmpW/6}
    \pgfmathsetmacro{\xg}{\qfdNH + (\sg + 0.5)*\qfdCmpW/6}
    \pgfmathsetmacro{\xs}{\qfdNH + (\sh + 0.5)*\qfdCmpW/6}
    \coordinate (po-\r) at (\xo, {-\r + 0.5});
    \coordinate (pr-\r) at (\xt, {-\r + 0.5});
    \coordinate (pf-\r) at (\xf, {-\r + 0.5});
    \coordinate (pp-\r) at (\xg, {-\r + 0.5});
    \coordinate (ps-\r) at (\xs, {-\r + 0.5});
  }

  % Pass 2: profile lines per alternative. Drawn before markers so the
  % dots sit on top of (not under) the line endpoints.
  \draw[qfdalt1ln] (po-1) \foreach \r in {2,...,\qfdNW} { -- (po-\r) };
  \draw[qfdalt2ln] (pr-1) \foreach \r in {2,...,\qfdNW} { -- (pr-\r) };
  \draw[qfdalt3ln] (pf-1) \foreach \r in {2,...,\qfdNW} { -- (pf-\r) };
  \draw[qfdalt4ln] (pp-1) \foreach \r in {2,...,\qfdNW} { -- (pp-\r) };
  \draw[qfdalt5ln] (ps-1) \foreach \r in {2,...,\qfdNW} { -- (ps-\r) };

  % Pass 3: markers on top of the lines.
  \foreach \r in {1,...,\qfdNW} {
    \node[qfdalt1mk] at (po-\r) {};
    \node[qfdalt2mk] at (pr-\r) {};
    \node[qfdalt3mk] at (pf-\r) {};
    \node[qfdalt4mk] at (pp-\r) {};
    \node[qfdalt5mk] at (ps-\r) {};
  }

  % ---------- Manual legend (5 alternatives, placed right of zones) ----------
  \pgfmathsetmacro{\qfdLegX}{\qfdNH + \qfdCmpW + 0.7}
  \begin{scope}[shift={(\qfdLegX, \qfdHdrH - 0.4)}]
    \draw[qfdmed, rounded corners=2pt]
      (-0.15, 0.4) rectangle (5.1, -7.50);
    % Relations
    \node[anchor=west, font=\footnotesize\bfseries] at (0, 0.1)
      {Relation};
    \draw[qfdthin] (0, -0.15) -- (4.95, -0.15);
    \node[qfdstrong] at (0.22, -0.5)  {};
      \node[anchor=west] at (0.5, -0.5)  {Strong (9)};
    \node[qfdmod]    at (0.22, -0.95) {};
      \node[anchor=west] at (0.5, -0.95) {Medium (3)};
    \node[qfdweak]   at (0.22, -1.4)  {};
      \node[anchor=west] at (0.5, -1.4)  {Weak (1)};
    % Correlation
    \node[anchor=west, font=\footnotesize\bfseries] at (0, -2.10)
      {Correlation};
    \draw[qfdthin] (0, -2.35) -- (4.95, -2.35);
    \node[anchor=west] at (0, -2.70) {{$+\!+$}\quad very positive};
    \node[anchor=west] at (0, -3.05) {{$+$\phantom{$+$}}\quad positive};
    \node[anchor=west] at (0, -3.40) {{$-$\phantom{$-$}}\quad negative};
    \node[anchor=west] at (0, -3.75) {{$-\!-$}\quad very negative};
    % Perception
    \node[anchor=west, font=\footnotesize\bfseries] at (0, -4.20)
      {Perception};
    \draw[qfdthin] (0, -4.45) -- (4.95, -4.45);
    \draw[qfdalt1ln] (0.05, -4.80) -- (0.45, -4.80);
      \node[qfdalt1mk] at (0.25, -4.80) {};
      \node[anchor=west, font=\bfseries] at (0.55, -4.80)
        {Typoena (shipped, measured)};
    \draw[qfdalt2ln] (0.05, -5.25) -- (0.45, -5.25);
      \node[qfdalt2mk] at (0.25, -5.25) {};
      \node[anchor=west] at (0.55, -5.25) {reMarkable 2 + Type Folio};
    \draw[qfdalt3ln] (0.05, -5.70) -- (0.45, -5.70);
      \node[qfdalt3mk] at (0.25, -5.70) {};
      \node[anchor=west] at (0.55, -5.70) {Freewrite Traveler};
    \draw[qfdalt5ln] (0.05, -6.15) -- (0.45, -6.15);
      \node[qfdalt5mk] at (0.25, -6.15) {};
      \node[anchor=west] at (0.55, -6.15) {Freewrite Smart Typewriter};
    \draw[qfdalt4ln] (0.05, -6.60) -- (0.45, -6.60);
      \node[qfdalt4mk] at (0.25, -6.60) {};
      \node[anchor=west] at (0.55, -6.60) {Pomera DM250};
    \node[anchor=west, font=\scriptsize\itshape] at (0, -7.15)
      {0 = poor, 5 = excellent};
  \end{scope}

\end{qfdhouse}
\end{document}
```

## 1. Customer requirements (the WHATs)

What a user (= me) values about the device, with importance weights on a
1–10 scale. Source columns point at the doc the requirement comes from.

| ID  | Requirement                                                                        | Weight | Source                                                                                                                             |
| --- | ---------------------------------------------------------------------------------- | :----: | ---------------------------------------------------------------------------------------------------------------------------------- |
| W1  | Sub-second visible response to typing                                              |   10   | [product → Write](v0.1-mvp-product.md#user-stories), [README → UX](../README.md#ux-boundaries-set-by-the-medium)                   |
| W2  | **Publishing** is one deliberate action away                                       |   9    | [product → Publish](v0.1-mvp-product.md#user-stories), [CONTEXT → Publish](../CONTEXT.md#user-facing-actions)                      |
| W3  | Pulling power never corrupts the file                                              |   10   | [product → Recover](v0.1-mvp-product.md#user-stories), [acceptance](v0.1-mvp-product.md#acceptance-criteria)                       |
| W4  | Provisioning never interrupts a writing session                                    |   7    | [product → Provisioning](v0.1-mvp-product.md#provisioning-build-time-dev-only), [macroplan → v0.9](macroplan.md#v09--robustness--) |
| W5  | Quick boot to a writing cursor                                                     |   6    | [product → acceptance](v0.1-mvp-product.md#acceptance-criteria) (≤ 5 s)                                                            |
| W6  | Long sessions without crash / lag / drift                                          |   9    | [product → acceptance](v0.1-mvp-product.md#acceptance-criteria) (1 h soak)                                                         |
| W7  | Nothing on the device competes with prose                                          |   8    | [README → vision](../README.md#vision)                                                                                             |
| W8  | The UI never moves except when I move it                                           |   7    | [README → UX](../README.md#ux-boundaries-set-by-the-medium)                                                                        |
| W9  | Codebase absorbs the planned roadmap without rewrite                               |   8    | [macroplan](macroplan.md)                                                                                                          |
| W10 | I can repair or fork it with hobbyist tools                                        |   5    | [README → vision](../README.md#vision)                                                                                             |
| W11 | Multi-day battery life (v0.8 onward)                                               |   4    | [macroplan → v0.8](macroplan.md#v08--power-battery--sleep--)                                                                       |
| W12 | Local-only file scope coexists with git scope (v0.5+)                              |   5    | [README → scopes](../README.md#vision), [macroplan → v0.5](macroplan.md#v05--file-palette--multi-file--)                           |
| W13 | Typography sets a writing-tool tone — typewriter or developer editor, never gadget |   7    | [macroplan → v1.0](macroplan.md), [README → UX](../README.md#ux-boundaries-set-by-the-medium)                                      |
| W14 | I can carry the device and write away from a desk                                  |   8    | [macroplan → v0.8](macroplan.md#v08--power-battery--sleep--), [README → hardware](../README.md#hardware)                           |
| W15 | A first-time user reaches writing without developer tools                          |   7    | [wizard](v0.9-onboarding-wizard.md) (zero-computer path), [installer](../installer/DESIGN.md) (one-command Mac path)               |

---

## 2. Engineering characteristics (the HOWs)

Measurable attributes: performance metrics of the device's functions
(below), or properties of its firmware artifact, memory layout, and build
process. See [`../GLOSSARY.md`](../GLOSSARY.md) for the ontology layers
(WHAT / Function / Characteristic / Metric / Target). Targets are v0.1
unless noted. Direction column shows what "better" looks like
(↑ higher, ↓ lower, → fixed).

### Functions

The device performs these. The HOW rows below measure quality attributes
of these functions, or of artifacts they produce.

| Function  | Transformation                             |
| --------- | ------------------------------------------ |
| Type      | keypress → glyph rendered + buffer mutated |
| Save      | dirty buffer → persisted file on SD        |
| Publish   | persisted file → commit on remote          |
| Recover   | degraded file state → readable file        |
| Boot      | power-on → cursor ready                    |
| Provision | uninitialized device → configured device   |

**Provision** was build-time-only in v0.1 ([ADR-005], [ADR-007]); as of the
v0.9 wizard (slices 0–5a hardware-verified 2026-07-16,
[`v0.9-onboarding-wizard.md`](v0.9-onboarding-wizard.md)) it is a runtime
function with **two peer realisations**: the on-device wizard (keyboard +
panel + the user's phone — no computer) and the macOS installer
([`../installer/DESIGN.md`](../installer/DESIGN.md)), which prepares the same
SD card from a Mac. Both write the same artifact (`/sd/typoena.conf` + a
cloned `/sd/repo`) and both authenticate through the Typoena GitHub App
device flow. Sub-functions referenced
inside HOW names: **Render** (buffer → e-ink frame, inside Type),
**Reconnect** (network outage → restored, inside Publish).

### Characteristics

| ID  | Characteristic                                 | Dir | v0.1 target              | v1.0 target         |
| --- | ---------------------------------------------- | :-: | ------------------------ | ------------------- |
| H1  | Type latency (keypress → glyph)                |  ↓  | ≤ 400 ms §               | ≤ 300 ms §          |
| H2  | Partial-refresh region area per keystroke      |  ↓  | ≤ 1 text line (~22 px h) | same                |
| H3  | Full-refresh cadence (clears ghosting)         |  →  | 1 per 64 partials        | tuned by panel temp |
| H4  | Boot latency (cold)                            |  ↓  | ≤ 5 s                    | ≤ 3 s †             |
| H5  | Continuous-typing endurance (no drop, no leak) |  ↑  | ≥ 1 h                    | ≥ 8 h               |
| H6  | Publish reliability (network up)               |  ↑  | ≥ 95 %                   | ≥ 99 %              |
| H7  | Publish latency (one file)                     |  ↓  | ≤ 30 s ‡                 | ≤ 10 s ‡            |
| H8  | Save durability (post-confirm power loss)      |  →  | 100 %                    | 100 %               |
| H9  | Heap headroom during Publish                   |  ↑  | ≥ 1 MB PSRAM free at peak ¶ | same             |
| H10 | Firmware binary size                           |  ↓  | ≤ 2 MB                   | ≤ 1.5 MB            |
| H11 | Stack budget across all tasks                  |  ↓  | ≤ 128 KB (sum) ∥         | same                |
| H12 | Network reconnect time (transient outage)      |  ↓  | ≤ 30 s                   | ≤ 10 s              |
| H13 | Idle / typing / Publish current draw           |  ↓  | measured only            | sized for >2 days   |
| H15 | Build time (clean, release)                    |  ↓  | ≤ 7 min                  | ≤ 5 min             |
| H16 | Onboarding duration (blank card → writing cursor) | ↓ | ≤ 10 min (v0.9, unmeasured) | same            |

† **Boot latency, measured 2026-07-11:** cold boot is **4258 ms**, so the ≤ 5 s
v0.1 target is met. The ≤ 3 s v1.0 target is assessed **marginal-to-unreachable** —
one ~1.9 s full refresh is unavoidable at cold boot (the `0x26` "previous" bank is
garbage until the first full paint), an e-ink floor rather than a tuning knob.
The 2026-07-14 boot restructure (async splash refresh, palette file-walk moved
to a background thread) held cursor-ready at ~4.2 s even with the full git
build and the 1100-file card walk — the walk now lands mid-session instead of
blocking boot. Breakdown + levers:
[`notes/boot-time-budget.md`](notes/boot-time-budget.md).

‡ **Publish latency, re-measured on the real repo 2026-07-13/14:** on the
author's real notes repo (~63 k objects), a cold `:gp` is **~24 s** and a warm
clean publish **~19 s** — inside the ≤ 30 s v0.1 target, but the earlier toy-repo
figures (~16 s cold / ~10 s warm, 2026-07-11) turned out not to transfer:
publish cost scales with repo shape. The mix also inverted: the push leg is
**5.9 s** (TLS session resumption); the splice-commit dominates at **10.3 s**
for one depth-4 file, and the convicted residual is **FAT linear directory
scans** (~0.1 ms/entry, `objects/` fan-out ≈ 256 dirs — see
[`tradeoff-curves/sync-commit-staging.md`](tradeoff-curves/sync-commit-staging.md)).
The ≤ 10 s v1.0 target is **not met on deep paths** (root-level warm ≈ 12–13 s);
the lever is pack-not-loose object writes, deferred to a perf pass. History:
[`notes/sync-latency.md`](notes/sync-latency.md),
[`kaizen/real-repo-sync.md`](kaizen/real-repo-sync.md).

§ **Type latency — two tiers, re-read 2026-07-14.** The ~630 ms figure measured
2026-07-11 is the **full-area partial** (deletes, caret moves, mode flips, the
splash→editor swap), not the additive typing path: per-keystroke typing rides
the **windowed-Y partial** (~10 rows), projected at **~100–130 ms** from the
floor+slope model — inside the ≤ 400 ms target, bench confirmation from the
on-device refresh log still pending
([`tradeoff-curves/epd-refresh-latency.md`](tradeoff-curves/epd-refresh-latency.md)).
The v0.1 target history stands: relaxed from ≤ 200 ms to ≤ 400 ms (the original
was tighter than [ADR-003]'s own accepted "~200–300 ms" e-ink cost); v1.0 reset
from ≤ 150 ms to ≤ 300 ms. The open item is now the **erase/caret tier**
(~630 ms full-area partial per event), not additive typing.

¶ **Heap during Publish — measured and re-plumbed 2026-07-13.** The ≥ 1 MB
PSRAM bar is now **met** (min-ever 4.5 MB free on the first real-repo push,
run 9) — but only after capping libgit2's mmap working set (mwindow
64 KB/1.5 MB, was 256 KB/4 MB; whole-file `.idx`/midx maps sit **outside** that
budget) and the odb cache at 1 MB. The learning that renamed this row: PSRAM
was the wrong sole pool to watch — **internal DRAM** is now the scarcer
resource (min-ever ~2.1 KB during TLS send; the mbedTLS
`EXTERNAL_MEM_ALLOC` move and interning the palette file list to one PSRAM
blob were both forced by internal-DRAM exhaustion). Watched via the
`log_push_heap` telemetry in `git_sync`.

∥ **Stack budget revised ≤ 80 KB → ≤ 128 KB (2026-07-16).** The old target was
priced for the pre-libgit2 five-thread guess (76 KB). Shipped reality: git
thread **96 KB** (libgit2 needs the headroom, [ADR-004] outcome) + USB pumps
4 + 8 KB + background file-walk 16 KB = **124 KB explicit**; UI/render run on
the main task and Wi-Fi is owned by the git thread, so there are no separate
ui/render/wifi stacks. Comfortable in 512 KB SRAM either way; the target
moved to follow the architecture, not the other way around.

---

## 3. House of Quality — WHATs × HOWs

This section reads the House (the diagram is at the top of this document): §1's
WHATs (rows) × §2's HOWs (columns), each cell scoring how strongly a
characteristic advances a requirement (9 / 3 / 1 / blank). The roof carries the §4 HOW-vs-HOW correlations; the basement carries the
v0.1 targets (from §2), the weighted-vote sums `Σ = Σ(W weight × cell strength)`,
and rounded relative weights. The right-hand zone scores five products against
the WHATs (0–5): the four competitors are **guessed, not measured**, while the
Typoena column is the **shipped, measured device** (rebased 2026-07-16; see
[Perception scores](#perception-scores-guessed)). The Σ totals quoted in the
priority list below come from the basement.

### Reading the house

- **Importance (left column)** is the raw 1–10 weight from §1, not a normalised
  %, so adding stays cheap when a WHAT shifts. Sum of weights is 110; treat each
  unit as ~0.91 % if you want a percentage view.
- **Roof** carries the §4 symbols translated into classical QFD glyphs:
  `++` strong reinforcement (`◎`), `+` mild reinforcement (`○`), `−` mild
  conflict (`×`), `−−` strong conflict (`⊗`).
- **Basement rows** are: v0.1 target → column sum (`Σ` of `weight × strength`) →
  relative weight as integer % of total (1627). Rounded relative weights sum
  to ~100 (102 with 15 columns' rounding).
- **H7, H10, H15** (Publish latency, binary size, build time) sit at the bottom
  of the basement, knowingly-paid costs per §7, not signals to optimise harder.

### Top engineering priorities (from importance)

1. **H9 — heap during push** (193). libgit2 pack + rope + TLS all
   share the same arena; [ADR-001] and [ADR-004] trade binary size for ecosystem
   so this became the watched metric — and the watch paid off: the 2026-07-13
   real-repo push campaign found libgit2's mmap working set as the consumer,
   capped it (mwindow 64 KB/1.5 MB, odb 1 MB), and shifted the live worry to
   internal DRAM (see §2 ¶). The umbrella typography WHAT (W13)
   keeps a fixed-size glyph-cache load on top of that arena pressure.
2. **H2 — partial-refresh region area** (177). Bound how many pixels the
   panel has to flip per keypress; [ADR-003] is the hardware-side answer.
3. **H8 — save durability** (156). Atomic-rename + fsync; FAT's weakness
   is acknowledged in [ADR-007] and mitigated, not designed around. H8's
   voter base spans W3 (power-loss correctness), W6 (long sessions),
   W12 (file scopes), and W14 (carrying = unclean shutdowns) — the
   fourth voter is what lifts H8 into the top three by arithmetic alone.
4. **H12 — network reconnect time** (160). Mobile use is the chief driver
   (W14 + W2 + W4 + W6, now joined by W15's clone leg); TLS session
   resumption (2026-07-14, second vendored delta in
   `esp_mbedtls_stream.c`) is the shipped answer — it cut the rejected-push
   reconcile fetch from ~30 s to ~5 s. Previously below the top six on a
   stationary v0.1 reading; W14 promotes it.
5. **H1 — Type latency** (148). The single most user-visible number;
   [ADR-002] and [ADR-003] are co-conspirators.
6. **H3 — full-refresh cadence** (144). The ghosting/flash tradeoff; lives
   in the render layer.

H13 (current draw, 137) sits at #7, close to the top-six cutoff because
W14 promotes the "wall-power for v0.1, measure first" stance from
acknowledged tradeoff to watched metric. The v0.1 "measured only" target
(§2) is still right; what changes is that bench multimeter readings (§6)
gain a second audience — sizing the v0.8 cell against a real portability
target, not just informing ADR-008's deferral.

H6 (Publish reliability, 134) sits just below the top six. Its ADR ownership
is [ADR-004] — whose spike-7 kill-switch **fired** (2026-07-06, gix has no
HTTPS push; the shipped transport is libgit2) — plus [ADR-005] token auth.
The matrix simply reads W14's mobile-use voter as a louder signal for
reconnect (H12) than for the Publish transport itself.

H16 (onboarding duration, 63) enters at the bottom of the table rather than
the top: W15 is its only strong voter, so the house correctly reads it as a
first-run, once-per-device characteristic — important to the product's reach
(it is what typoena.dev's "just power it on" sells), not to the daily writing
loop. Its budget row lives in §6 because it is still unmeasured.

**Why H8 ranks where it does.** Pre-W14, HoQ totals rewarded characteristics
that touch many WHATs over characteristics that absolutely matter for one WHAT.
W3 ("Pulling power never corrupts the file", weight 10) was H8's
strongest single voter, but H8 still sat at #6 because its base was
narrow. W14's "carrying = bumps = unclean shutdowns" widens H8's voter
base and pushes it to #3 by arithmetic. §6's "table-stakes correctness"
override is no longer the load-bearing argument for H8's prominence —
its acceptance-criteria override for H4/H5 still is. See §6.

The bottom three (H7 Publish latency, H15 build time, H10 binary size) are real
costs but ones we knowingly took on ([ADR-001]) and are not in the critical
path of user experience. The tightened H15 v0.1 target (≤ 7 min) reflects
user preference for faster iteration, not matrix-derived priority; if it
pushes back against [ADR-001]'s "+5–10 min" pricing, the target moves
before the runtime decision does.

### Perception scores (guessed)

Five products on the 0–5 scale, scored against each WHAT. Reference
configurations: **reMarkable 2 + Type Folio**, **Freewrite Traveler**,
**Freewrite Smart Typewriter**, **Pomera DM250** (DM250 has a reflective
monochrome LCD, not e-ink — flagged in W1 / W8). The Typoena column is the
**shipped device as of 2026-07-16** (v0.1 delivered 2026-07-11, v0.5–v0.7
delivered 2026-07-12/14, v0.9 wizard slices 0–5a hardware-verified), rebased
on measured hardware results and lived use; the four competitors remain
single-rater guesses. Three Typoena moves since the 2026-07-11 rebase: W1
2→3 (the ~630 ms figure turned out to be the erase/caret tier — additive
typing rides the ~100–130 ms windowed-Y partial, projected, bench confirm
pending), W12 3→4 (v0.5 multi-file + Local scope shipped on device), and the
new W15 row (wizard + installer + install.sh one-liner).

Freewrite Traveler scores assume the
[Sailfish firmware](https://getfreewrite.com/blogs/writing-success/freewrite-sailfish-firmware)
(released 2025-11-19), which rewrote the OS in Rust, cut keystroke latency
40–100 %, and trimmed power draw −30 % typing / −50 % idle on both
Traveler and Smart Typewriter Gen 3. Three rows rescored upward as a
result: W1 Traveler 3→4 / Smart 2→3 (Smart's larger panel still trails
Traveler by one notch), W5 both 3→4 (boot accelerated, no published
number), W9 both 1→2 (Rust rewrite explicitly unblocked features that
JS could not carry; still closed so neither reaches reMarkable's
hackable-Linux 3).

| ID  | WHAT (truncated)                                  | Typoena | reM. | Frw.T | Frw.S | Pom. | Rationale (shortest defensible)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| --- | ------------------------------------------------- | :-----: | :--: | :---: | :---: | :--: | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| W1  | Sub-second response to typing                     |    3    |  1   |   4   |   3   |  5   | Typoena additive typing rides the windowed-Y partial, ~100–130 ms projected (2026-07-14 re-read; bench confirm pending) — competitive with the Freewrites — but erase/caret events still pay the ~630 ms full-area partial, so 3 not 4; reMarkable e-ink visibly laggy on a typing-focused device — tested less responsive than Smart Typewriter, and latency is so load-bearing for W1 that it earns a 1 not a 2; both Freewrites post-Sailfish trimmed latency 40–100 % (Frw.T plausibly inside 200 ms; Frw.S still trails by one notch on larger panel); Pomera LCD ~zero. |
| W2  | Publishing is one deliberate action away          |    5    |  4   |   4   |   4   |  2   | `:gp` atomic (one command, splice → commit → push, rejected-push replay included — verified on device 2026-07-14); reMarkable + Freewrite cloud-sync is one-tap but not git; Pomera = USB/SD copy or QR transfer.                                                                                                                                                                                                                                                                                                                          |
| W3  | Pulling power never corrupts the file             |    4    |  4   |   2   |   2   |  2   | Typoena: atomic-rename + fsync, plus the dirty-path journal at `/sd/.typoena-dirty` making an interrupted Publish power-pull-safe (2026-07-13); the actual power-pull test is still deferred to v0.9, so 4 not 5. reMarkable journals. Freewrite + Pomera: forum reports of corruption on yank.                                                                                                                                                                                                                                            |
| W4  | Provisioning never interrupts writing             |    5    |  2   |   2   |   2   |  5   | Typoena: config is read once at boot from `/sd/typoena.conf`; reconfiguration lives behind `:setup` (reboot → reset menu), never mid-session. reM/Frw need Wi-Fi + account. Pomera: literally none.                                                                                                                                                                                                                                                                                                                                       |
| W5  | Quick boot to a writing cursor                    |    4    |  3   |   4   |   4   |  5   | Typoena measured 4.26 s cold (2026-07-11). reMarkable cold-boots ~20 s (great from sleep). Both Freewrites accelerated post-Sailfish (no published number; were ~10–15 s e-ink wake). Pomera ~3 s.                                                                                                                                                                                                                                                                                                                                         |
| W6  | Long sessions without crash / lag / drift         |    4    |  3   |   4   |   4   |  5   | Typoena: 1 h soak attested 2026-07-11 (real use, no crash / lag / leak) — one proven hour vs rivals' years, so 4 not 5. Freewrite famously stable (both variants). Pomera firmware is decades-mature.                                                                                                                                                                                                                                                                                                                                      |
| W7  | Nothing on the device competes with prose         |    5    |  2   |   5   |   5   |  5   | reMarkable has apps, menus, drawing, PDFs. Freewrite + Pomera are single-purpose; Typoena by design.                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| W8  | The UI never moves except when I move it          |    4    |  3   |   4   |   4   |  5   | reMarkable animates more; Typoena uses dirty-rects; Freewrites minimal motion; Pomera near-static LCD.                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| W9  | Codebase absorbs the planned roadmap              |    4    |  3   |   2   |   2   |  1   | Modular Rust Typoena; reMarkable is hackable Linux; both Freewrites carry Sailfish (Rust rewrite explicitly unblocked features JS could not carry) but closed; Pomera closed firmware.                                                                                                                                                                                                                                                                                                                                                     |
| W10 | I can repair or fork it with hobbyist tools       |    5    |  4   |   2   |   2   |  1   | Typoena: open BOM + ESP32. reMarkable: rooted Linux + community ROMs. Freewrite + Pomera: closed.                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| W11 | Multi-day battery life (v0.8 onward)              |    1    |  5   |   5   |   5   |  4   | Typoena v0.1 = wall-powered (battery deferred). reMarkable + both Freewrites legendary (~4 weeks; Sailfish trimmed −30 % typing / −50 % idle). Pomera ~24 h.                                                                                                                                                                                                                                                                                                                                                                               |
| W12 | Local-only files coexist with git scope           |    4    |  1   |   2   |   2   |  3   | Typoena: shipped in v0.5 (on-device 2026-07-12) — `/sd/local` never publishes, palette walks both scopes; 4 not 5 while the scope model has one shipped week of lived use. reMarkable cloud-only. Freewrites have local + Postbox but no VCS. Pomera = pure local.                                                                                                                                                                                                                                                                         |
| W13 | Typography sets a writing-tool tone               |    3    |  5   |   2   |   2   |  2   | Typoena v0.1: single mono (serif option in v1.0). reMarkable: rich type rendering. Freewrite + Pomera: utilitarian.                                                                                                                                                                                                                                                                                                                                                                                                                        |
| W14 | I can carry the device and write away from a desk |    2    |  4   |   5   |   1   |  5   | Typoena still wall-powered (ADR-008) — desk-bound until v0.8's battery, though the parametric case (`hardware/case/`, OpenSCAD) now exists. reMarkable + Type Folio bag-friendly with bulk. Freewrite Traveler is the form-factor reference (~1.6 lb, folds). Smart Typewriter ~5 lb, desk-bound. Pomera DM250 pocketable foldable.                                                                                                                                                                                                        |
| W15 | First-time setup without developer tools          |    4    |  2   |   3   |   3   |  5   | Typoena: two verified paths — on-device wizard (Wi-Fi scan-pick, QR device-flow sign-in, repo pick + shallow clone; slices 0–5a on hardware 2026-07-16) and the `curl … \| sh` installer (checksum-verified, no toolchain); 4 not 5 while factory-reset/repo-switch are on-device-pending and repos > ~30 MB are refused. reMarkable needs account + companion app. Freewrites need a Postbox account. Pomera: no setup at all.                                                                                                            |

**Totals** (sum across 15 WHATs, no weighting): Typoena 57, Pomera 55,
Freewrite Traveler 50, reMarkable 46, Freewrite Smart Typewriter 45.
History: Typoena 52→51 at the 2026-07-11 measurement rebase (W6 +1 soak,
W1 −2 on the ~630 ms reading), then 51→57 at the 2026-07-16 rebase (W1 +1
once the ~630 ms figure was re-read as the erase tier, W12 +1 on shipped
v0.5, W15 4 new); every product gained its W15 row (Pomera +5, Traveler and
Smart +3, reMarkable +2). Traveler pre-Sailfish 44; Smart pre-Sailfish 39;
reMarkable W1 dropped 3→2→1 across two rounds of author testing.
Typoena's lead over Pomera is two points and still hinges on the same
dimensions: W14 (portability) and W1's erase tier are where the tethered
e-ink device loses ground; v0.8 (battery) and a faster erase/caret path are
what widen it. The "Pomera + Wi-Fi + git + hackable BOM" framing from
`README.md` holds — and W15 is now measurable product surface (wizard +
installer), not aspiration.

Weighted totals (Σ score × W weight) tell the same story with more
contrast — left as exercise; the unweighted view is enough to read the
picture.

#### Caveats

- **Single-rater bias.** All fifteen rows are scored from the project
  author's POV. A reMarkable buyer would weight W11 (battery) at 10 and
  W12 (git) at 1, flipping the totals.
- **Configuration matters.** Freewrite Smart Typewriter and Traveler are
  both tracked; they diverge most on W1 / W5 because of display tech
  (Smart's larger panel is slower to refresh). Traveler is still the
  more direct competitor on form factor.
- **W3 / W6 Freewrite scores are anecdotal.** Forum reports, not bench
  data. Treat the 2 / 4 as "we'd need to test this" rather than fact.
- **No price column.** Typoena-as-BOM is materially cheaper than the
  competitors but cost is not a WHAT in §1, so it's absent here.
  Worth a row if a v0.x WHAT ever calls it out.

### Regenerating

The matrix cells (`\node[qfdrel/{S,M,W}]`), roof symbols (`C-i-j` slots),
and basement Σ + Rel% are native to this file — re-score directly in the
TikZ source, then update the §3 priority list and §4 conflict list in
the narrative above to match the new picture.

When §1 or §2 changes:

1. Importance column → §1 weight column.
2. HOW titles + v0.1 targets → §2 target column.
3. Recompute basement Σ for any HOW whose column changed: per-cell
   contribution = `(W weight) × (cell strength: 9 / 3 / 1 / 0)`.
4. Recompute relative weight: each Σ ÷ total Σ × 100, rounded to integer
   percent.

Perception scores are **not** derived from §1/§2 — they live only in
this file. Update them when (a) a competitor ships a relevant change,
(b) measurement replaces a guess, or (c) a WHAT is added/removed in §1.
Each score keeps its one-line rationale in the table above.

If a renderer rejects the `tikz` fence, the file is still readable as
source — the placement comments name each WHAT, HOW, and cell. The
perception-scores table above is the human-readable fallback for the
right-hand zone of the diagram.

---

## 4. Roof — HOW-vs-HOW tradeoffs

The roof shows where pushing one characteristic pushes another the wrong way.
ASCII glyphs (with classical QFD equivalents): **`++`** strong
reinforcement (`◎`), **`+`** mild reinforcement (`○`), **`−`** mild
conflict (`×`), **`−−`** strong conflict (`⊗`). The 14×14 roof matrix is
in the §3 diagram; the cells that actually shape the design are called out
below.

### Conflicts that actually shape the design

- **H1 latency ↔ H3 refresh cadence** (mild). More partial refreshes per
  second pile up ghosting faster, demanding earlier full refreshes —
  visible flashes that hurt H8 perception and H1 burst behaviour. The
  [ADR-003] strip aspect is the structural answer: a small framebuffer makes
  _both_ cheaper, not one at the expense of the other. The runtime answer
  is render §H3: schedule full refreshes on idle ≥ 1 s (v0.1 tech doc). The
  rows-vs-latency cost model behind this tradeoff — full / full-area-partial /
  windowed-Y — is in
  [`tradeoff-curves/epd-refresh-latency.md`](tradeoff-curves/epd-refresh-latency.md).
- **H9 heap ↔ H10 binary size** (strong). std + libgit2 + mbedtls inflate
  both. We chose to spend on these ([ADR-001], [ADR-004]) because 16 MB flash
  and 8 MB PSRAM make them affordable. Spike 7's kill-switch fired for a
  different reason than feared (gix had no HTTPS push, not a heap failure),
  and the heap fight then happened on libgit2's side: the 2026-07-13 push
  campaign traced full-PSRAM exhaustion to its mmap working set and settled
  it with hard caps (mwindow 64 KB/1.5 MB, odb cache 1 MB — §2 ¶).
- **H9 heap ↔ H5 soak** (strong). A long writing session grows the rope
  and the glyph cache; Publishing on top can OOM. Mitigations shipped: 256 KB
  file cap (v0.1 tech doc), the persistent two-frame draw in `main.rs`
  (repaints never allocate — added after a mid-push `HalfPageUp` OOM-aborted
  the UI thread, run 4), and the palette file list interned to one PSRAM
  blob (was 182 KB of internal DRAM).
- **H6 Publish reliability ↔ H12 network reconnect** (reinforcing). Both come
  from the same network stack; the TLS session-resumption vendored delta
  (2026-07-14) proved the correlation — added for reconnect cost, it also
  removed the idle keep-alive push failure's sting (run 8's `SSL Generic
  error` during the 31 s marking gap; durable fix — reconnect-on-stale in
  the http layer — still open).
- **H12 reconnect ↔ H16 onboarding** (reinforcing). The wizard's clone leg
  rides the same TLS + Wi-Fi bring-up as Publish; every second shaved off
  connect/reconnect shortens first-run too (ls-refs fast path, session
  resumption).
- **H10 binary ↔ H15 build time** (strong). std builds are slow. Accepted
  in [ADR-001] — refactor leverage is the long-term payoff, not the
  per-build seconds.
- **H4 boot ↔ H10 binary** (mild). Larger binary = slower flash load.
  Affordable at our size class but worth watching as features land.
- **H11 stacks ↔ H13 current draw** (mild, future). Idle threads draw
  little but never zero; a future light-sleep policy (v0.8) wants them
  parked. W14's portability outcome raises the value of that policy
  from "battery hygiene" to "the thing that lets the device leave the
  desk."
- **W13 typography ↔ H9 heap + H10 binary** (mild, future). Achieving a
  writing-tool tone needs room for glyph caches and font assets. Not
  load-bearing in v0.1 (one mono font), but the v1.0 tone goal is why H9
  and H10 keep slack rather than being squeezed to the minimum.
- **Tightened H15 ↔ [ADR-001]** (mild). Pulling v0.1 build time from
  ≤ 10 min to ≤ 7 min eats into [ADR-001]'s accepted "+5–10 min" cost.
  Worth aiming at via cargo profile / vendor LTO / crate-graph trims;
  worth giving up before reversing [ADR-001].

---

## 5. HOW → Component mapping (Phase 2)

Which subsystem owns the delivery of each characteristic. Cells are which ADR
constrains the choice.

Components (with anchoring ADR):

| ID  | Component                            | ADR                   |
| --- | ------------------------------------ | --------------------- |
| C1  | ESP32-S3-N16R8 SoC                   | [ADR-001], [ADR-008]  |
| C2  | `esp-idf-rs` (std) + ESP-IDF         | [ADR-001]             |
| C3  | `std::thread` + `crossbeam-channel`  | [ADR-006]             |
| C4  | PSRAM allocator wrapper              | [ADR-001]             |
| C5  | GDEY0579T93 + DESPI-c579 panel       | [ADR-003]             |
| C6  | `embedded-graphics` + e-paper driver | [ADR-002], [ADR-003]  |
| C7  | Custom widget / dirty-rect layer     | [ADR-002]             |
| C8  | `ropey` rope buffer                  | [ADR-001] (ecosystem) |
| C9  | TinyUSB host (`esp-idf` bindings)    | [ADR-009]             |
| C10 | FAT on microSD (own SPI3 host)       | [ADR-007], [ADR-012]  |
| C11 | LittleFS on internal flash           | [ADR-007]             |
| C12 | `libgit2` (`git2`, esp-idf CMake component + vendored mbedTLS stream) | [ADR-004] (kill-switch outcome) |
| C13 | mbedtls TLS (via ESP-IDF)            | [ADR-005]             |
| C14 | HTTPS + GitHub token auth (PAT or App device-flow `ghu_`) | [ADR-005], [ADR-011]  |
| C15 | eFuse-derived encryption key         | [ADR-005], [ADR-007]  |
| C16 | USB-C wall PSU                       | [ADR-008]             |
| C17 | `conf` + `wizard` crates (on-device onboarding) | [wizard](v0.9-onboarding-wizard.md) |
| C18 | macOS installer (ratatui card provisioner) | [installer/DESIGN.md](../installer/DESIGN.md) |
| C19 | typoena.dev + `install.sh` one-liner | (site repo, checksum-verified release download) |
| C20 | Typoena GitHub App (device-flow auth) | [wizard](v0.9-onboarding-wizard.md) |

HOW-to-component matrix (9 strong / 3 medium / 1 weak):

|           | C1 SoC | C2 std | C3 thr | C4 PSR | C5 EPD | C6 eg | C7 wid | C8 rope | C9 USB | C10 SD | C11 LFS | C12 git | C13 TLS | C14 tok | C15 efs | C16 PSU | C17 wiz | C18 inst | C19 site | C20 app |
| --------- | :----: | :----: | :----: | :----: | :----: | :---: | :----: | :-----: | :----: | :----: | :-----: | :-----: | :-----: | :-----: | :-----: | :-----: | :-----: | :------: | :------: | :-----: |
| H1 lat    |   3    |   1    |   9    |   3    |   9    |   9   |   9    |    3    |   9    |        |         |         |         |         |         |         |         |          |          |         |
| H2 area   |        |        |        |        |   9    |   9   |   9    |         |        |        |         |         |         |         |         |         |         |          |          |         |
| H3 cad    |        |        |        |        |   9    |   3   |   9    |         |        |        |         |         |         |         |         |         |         |          |          |         |
| H4 boot   |   3    |   9    |   3    |   1    |   3    |       |        |         |        |   9    |    3    |         |         |         |         |         |         |          |          |         |
| H5 soak   |   3    |   3    |   3    |   9    |   1    |       |        |    9    |   9    |   3    |         |    3    |    3    |         |         |         |         |          |          |         |
| H6 reli   |        |   3    |        |        |        |       |        |         |        |        |         |    9    |    9    |    9    |         |         |         |          |          |         |
| H7 lat    |        |        |   3    |   1    |        |       |        |         |        |   3    |         |    9    |    9    |         |         |         |         |          |          |         |
| H8 dura   |        |   3    |        |        |        |       |        |         |        |   9    |    9    |         |         |         |         |         |         |          |          |         |
| H9 heap   |   3    |   3    |        |   9    |        |       |        |    3    |        |        |         |    9    |    9    |         |         |         |         |          |          |         |
| H10 bin   |        |   9    |   1    |        |        |   3   |   3    |    3    |   3    |        |         |    9    |    3    |         |         |         |    1    |          |          |         |
| H11 stk   |        |        |   9    |        |        |       |        |         |   3    |        |         |    3    |         |         |         |         |         |          |          |         |
| H12 recon |   3    |   9    |        |        |        |       |        |         |        |        |         |         |    3    |         |         |         |         |          |          |         |
| H13 mA    |   9    |        |   1    |        |   9    |       |        |         |   3    |   3    |         |         |         |         |         |    9    |         |          |          |         |
| H15 build |        |   9    |        |        |        |       |        |         |        |        |         |    9    |    3    |         |         |         |         |          |          |         |
| H16 onb   |        |        |        |        |        |       |        |         |        |   3    |         |    9    |    3    |    3    |         |         |    9    |    9     |    3     |    9    |

### Read across, not down

- **C5/C6/C7** (panel + graphics + widget) are the single most leveraged
  cluster — they own H1, H2, H3 (the top of the priority list). [ADR-002]
  and [ADR-003] are the ADRs to keep most honest as v0.x progresses.
- **C12** (`libgit2`) is overloaded: H6, H7, H9, H10, H11, H15 — and now
  H16's clone leg — all touch it. [ADR-004]'s kill-switch **fired**
  (spike 7, 2026-07-06: gix had no HTTPS push) and the fallback became the
  component: `git2` vendored as an esp-idf CMake component with two
  deliberate C-side deltas (`esp_mbedtls_stream.c`: a double-free fix in the
  error path reported upstream, plus TLS session resumption). The overload
  prediction held — the real-repo push campaign (2026-07-13) was fought
  entirely inside C12's memory profile. [ADR-010] pins the _shape_ of the
  publish sequence; swapping the library under it never changed the user
  contract, exactly as designed.
- **C11** (LittleFS) is **still unused — and v0.9 decided against it for
  config**: the wizard/installer write plaintext `/sd/typoena.conf` on the
  card (one artifact both provisioning paths share, desktop-inspectable).
  C15 (eFuse key) is likewise unused; token-at-rest protection is the open
  [ADR-011]. C11's non-zero cells describe a possible future shape per
  [ADR-007], not shipped reality.
- **C17–C20** (wizard, installer, site, GitHub App) form the onboarding
  cluster that owns H16. Deliberate redundancy: C17 and C18 are peer paths
  to the same configured card, so neither is a single point of failure for
  W15 — a Mac user never touches the wizard; a computer-less user never
  touches the installer. C20 (device-flow auth) and C12 (shallow clone) are
  the shared spine of both.
- **C2** (std runtime) sits underneath almost everything, but it's the
  _enabler_ (H4 boot, H10 binary, H12 reconnect) rather than the bottleneck.
  Reversing [ADR-001] would force re-deciding [ADR-004], [ADR-005],
  [ADR-006], [ADR-007] all at once — they're a single decision in three
  drawers.

---

## 6. Critical performance budget

A curated rank, drawing from §3 importance and §4 conflicts, with one
deliberate override: acceptance-criteria critical paths (H4 boot,
H5 soak) move up regardless of weighted-vote spread. (Pre-W14 this list
also lifted H8 durability over its narrow voter base; W14 has widened
that base, so H8's #3 spot is now arithmetic — see §3.) These started as
the numbers spikes 2–7 had to validate; most are now measured on the
shipped device, so the last column reads as verdict → next lever.

| Rank | Characteristic         | Target                           | Watched on                          | Verdict / next lever                                                                                                                                                      |
| ---- | ---------------------- | -------------------------------- | ----------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1    | H2 region area         | ≤ 1 line per keypress            | on-device refresh log               | ✓ windowed-Y drives only the touched line's band; fallback (larger font) never needed                                                                                     |
| 2    | H9 heap (Publish)      | ≥ 1 MB PSRAM free at push peak   | `log_push_heap` telemetry           | ✓ run 9: min-ever 4.5 MB after mwindow 64 KB/1.5 MB + odb 1 MB caps; **new watch = internal DRAM** (min-ever ~2.1 KB during TLS send) — §2 ¶                              |
| 3    | H8 durability          | 100 % (post-confirm power loss)  | dirty journal + boot recovery       | Journal (`/sd/.typoena-dirty`) + `*.tmp` boot-recovery + stranded-commit replay shipped; the physical power-pull test is still owed (v0.9)                                |
| 4    | H1 Type latency        | ≤ 400 ms (revised from ≤ 200 ms) | refresh log (bench confirm pending) | Typing tier ~100–130 ms projected ✓; **erase/caret tier ~630 ms ✗** — the open lever is a cheaper erase path, not faster typing                                           |
| 5    | H6 Publish reliability | ≥ 95 % (network up)              | daily `:gp` use                     | Rejected-push → reconcile → replay → push cycle verified on device 2026-07-14; residual risk = stale keep-alive on long marking gaps (avoided via repack, not fixed)      |
| 6    | H3 cadence             | full every ~64 partials          | `FULL_REFRESH_EVERY = 64`           | ✓ holding; adjust per panel temperature; defer flash to idle ≥ 1 s                                                                                                        |
| 7    | H4 Boot latency        | ≤ 5 s (cold, to cursor)          | 4258 ms 2026-07-11 ✓                | Held ~4.2 s through the 2026-07-14 restructure (async splash, background walk); v1.0 ≤ 3 s lever left: memtest off (−0.74 s) — [boot-time-budget](notes/boot-time-budget.md) |
| 8    | H5 soak                | 1 h no leak / no drop            | 1 h real-use soak ✓ 2026-07-11      | Attested; re-run after heap-touching changes (the run-4 per-draw-alloc OOM was exactly this class)                                                                        |
| 9    | H16 onboarding         | ≤ 10 min (blank card → cursor)   | **unmeasured** — time a fresh run   | Wizard slices 0–5a verified on hardware but never wall-clocked; if over, levers = shallow-clone tuning, device-flow poll cadence, the deferred SoftAP companion           |

The two not-in-MVP rows but already-shaped-by-design:

| — | H13 current | Measured only in v0.1 | bench multimeter | Cell sizing for v0.8 is data-driven, not spec-sheet |
| — | H11 stacks | Sum ≤ 128 KB (was ≤ 80 KB) | measured: 124 KB explicit (git 96 + walk 16 + USB 4+8) | Target followed the shipped architecture — §2 ∥; re-price before adding threads |

---

## 7. Tradeoffs and their why, linked to ADRs

Plain-language summary of what we accepted in exchange for what.

| Tradeoff                                        | Got                                                                                                  | Paid                                                                                                                                                  | ADR       |
| ----------------------------------------------- | ---------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- | --------- |
| std (esp-idf-rs) over no_std (esp-hal)          | Heap, threads, VFS, mbedtls, room for a full git stack (proved out by libgit2)                       | +1 MB binary, +5–10 min builds                                                                                                                        | [ADR-001] |
| Custom widget layer over Ratatui                | Dirty-rects aligned to e-ink regions; 200 KB binary back                                             | 500 LoC we own and maintain                                                                                                                           | [ADR-002] |
| e-ink medium over FSTN / memory LCD / OLED      | Paper aesthetic; 0 W idle persistence; medium enforces writing posture                               | ~200–300 ms typing latency; periodic full-refresh flash (scroll worst-case)                                                                           | [ADR-003] |
| `libgit2` (`git2`) over `gitoxide` — the [ADR-004] kill-switch, fired 2026-07-06 | Working HTTPS push on-device; mature pack/transport code riding ESP-IDF's mbedTLS                    | FFI + a C build (esp-idf CMake component); two vendored C deltas to maintain (`esp_mbedtls_stream.c` double-free fix + TLS session resumption); an mmap profile that needed hard caps (mwindow, odb) | [ADR-004] |
| HTTPS + GitHub token over SSH                   | Simplest auth the device transport supports; App device-flow tokens (`ghu_`) ride the same header as a PAT, so wizard/installer sign-in changed nothing in the git path | Long-lived secret on device — now **plaintext in `/sd/typoena.conf`** (both provisioning paths write it; physical custody of the card is the control); encrypted-at-rest is the open [ADR-011]      | [ADR-005], [ADR-011] |
| `std::thread` over `embassy` or `tokio`         | Boring, debuggable, real stack traces; no exec to tune                                               | ~76 KB total stack across 5 tasks                                                                                                                     | [ADR-006] |
| FAT-on-SD + LittleFS-on-flash split             | Desktop can read SD; config survives SD reformat                                                     | Two filesystems to manage; FAT's power-loss weakness mitigated by atomic-rename                                                                       | [ADR-007] |
| Wall power for v0.1, battery deferred           | Measure real draw before sizing the cell                                                             | Tethered MVP; not the final aesthetic                                                                                                                 | [ADR-008] |
| USB host (TinyUSB) over BLE-HID                 | No radio contention with Wi-Fi during push; keyboard powered from the device                         | One more USB connector on enclosure                                                                                                                   | [ADR-009] |
| Atomic Publish (`:gp`, was `Ctrl-G`) + auto-timestamp commit message | One action, one outcome; matches the user's existing `gct` workflow; no modal prompt to slow H1 latency | Commit history is timestamp noise; the device authors replay commits the user never sees; reversal would break muscle memory                          | [ADR-010] |
| Splice commit over full index write             | Real-repo Publish exists at all: ~19–24 s vs 611 s / OOM on the index path; dirty-path journal makes it power-pull-safe | Desktop-side edits to the card are never committed by the device — hand-edits on a computer must be pushed from that computer                         | [sync-commit-staging](tradeoff-curves/sync-commit-staging.md) |
| Media stays in git, never on the card           | Killed `:gl`'s last OOM path; pull/apply touches text only; the repo stays whole for remote readers | Stale card media; phantom `git status` noise if the card is mounted on a computer — never hand-commit from the card                                   | (2026-07-14, `is_media_path`) |
| Shallow clone + ~30 MB repo gate at onboarding  | First-run clone fits device memory and minutes-scale patience                                        | Repos over the gate are refused at the repo-pick step (libgit2 has no partial clone, so tip media would download even if never written)               | [wizard](v0.9-onboarding-wizard.md) |
| Installer provisions the card, never flashes    | No USB-flash toolchain in the user path; devices ship pre-flashed; installer stays a small TUI      | Field firmware updates cannot lean on the installer — auto-update becomes a device-side problem (open, macroplan v1.x)                                | [installer/DESIGN.md](../installer/DESIGN.md) |
| `curl … \| sh` one-liner over app-store/dmg     | Zero-friction start from typoena.dev; checksum-verified download; quarantine handled                | Pipe-to-shell trust ask; macOS-only today; the site and the GitHub release become launch-path infrastructure to keep up                               | (site repo `install.sh`) |

### Conflicts left explicitly _unresolved_ by v0.1

These are the live tensions we are watching, not deciding harder:

- **FAT loose-object cost vs H7's v1.0 target.** The convicted residual of
  Publish latency is FAT's linear directory scans (~0.4 s per loose write
  against the 256-dir `objects/` fan-out), bounded at ≤ ~2 s per commit and
  **accepted** for now; the lever (pack-not-loose writes) waits for a perf
  pass. Until then the ≤ 10 s v1.0 H7 target is not honest for deep paths.
- **Keep-alive race vs H6.** Run 8's push died on a connection idled out
  during a long marking gap; repack shrank the gap so run 9 succeeded —
  the race is *avoided*, not fixed. Durable fix = reconnect-on-stale in the
  http layer; owed before H6's v1.0 ≥ 99 % is credible.
- **Token at rest ([ADR-011], open).** Both provisioning paths write the
  GitHub token plaintext to `/sd/typoena.conf`; physical custody of the
  card is the only control. Encrypted-at-rest (C15's eFuse key, C11) stays
  designed-but-unbuilt until the threat model outgrows "solo device in my
  home."
- **Onboarding reach vs simplicity.** The wizard types Wi-Fi passwords on
  the device and the installer is macOS-only; the SoftAP companion webapp
  (phone-driven hand-off) was chosen over BLE 2026-07-16 and **deferred**.
  Revisit when a non-Mac, non-patient user actually appears.
- **[ADR-007] vs H8.** Power loss between FAT rename and dir flush yields
  the previous saved version. We document this as expected behavior; it
  becomes a real bug only if soak testing shows it triggering on routine
  saves.
- **W13 typography paths.** v0.1 ships one mono font; v1.0's
  writing-tool-tone outcome admits two paths (mono = developer comfort,
  serif = typewriter feel). Not yet decided whether to ship both or one;
  decision deferred to the v1.0 design pass. Cost preview per added font:
  +H9 glyph-cache footprint, +H10 binary for embedded assets.
- **[ADR-008] vs W11+W14.** Wall power in v0.1 is now an explicit
  disappointment of two WHATs, not one (battery W11 + portability W14).
  The disappointment is bounded by [ADR-008]'s commitment to measure
  current draw on real hardware before sizing v0.8's cell — spec the
  cell against measured numbers, not against the spec sheet. The §3
  promotion of H13 (current draw) from #11 to #7 is the matrix
  registering this: bench multimeter readings now serve portability
  sizing as well as power profiling.

---

## 8. Inconsistencies spotted and fixed

- **[ADR-006] stack figure.** [ADR-006] previously said "~40 KB of stack
  space for task stacks" — but the v0.1 technical design's task table
  (`usb 8 + wifi 8 + ui 16 + render 12 + git 32`) sums to **76 KB**.
  Updated [ADR-006]'s Consequences section to reflect the actual budget
  and cross-reference the tech doc. The 76 KB figure still fits
  comfortably in the ESP32-S3's 512 KB internal SRAM, so no design
  change, just documentation accuracy.
- **Commit-message format triple-mismatch.** README said `git commit -m
"wip"`, the v0.1 product doc said `"wip <timestamp>"`, and the user's
  actual shell alias (`gct` / `git-commit-timestamp`) uses a pure ISO-8601
  timestamp with no `wip` prefix. Resolved by aligning all docs on `gct`
  and recording the decision as
  [ADR-010].
  Pulled the v0.7 roadmap item "Commit message prompt instead of hard-coded
  `wip`" — it's now contradicted by [ADR-010] and removed.
- **First-run flow vs. target user.** The v0.1 product doc described a
  captive-portal first-run, but the same doc names the v0.1 target user as
  the dev themselves ("Me. Solo."). Provisioning a solo-dev device through
  a captive portal is ceremony without a user. Resolved by switching v0.1
  to build-time env-var config (no NVS, no LittleFS, no AP mode); on-device
  provisioning is the v0.9 release that introduces non-dev users. Touches
  [ADR-005], [ADR-007], the v0.1 product + technical docs, and the v0.9
  roadmap entry.
- **Vocabulary leak.** Earlier docs used "commit" and "push" as if they
  were distinct user actions; the gct/[ADR-010] model collapses them into a
  single user-facing **Publish**. Resolved by introducing
  [`CONTEXT.md`](../CONTEXT.md) as the canonical glossary; user-facing text
  now uses **Save** and **Publish** only.
- **House of Quality column sums recomputed.** Earlier Σ row drifted from
  the matrix arithmetic — H1 listed 138 but sums to 148; H8 147 vs 132;
  H9 162 vs 172; H13 74 vs 65; smaller deltas elsewhere. Recomputed all
  sums from the cells. H8 dropped from #3 to #6 — a "fewer WHAT voters"
  artifact, not a signal that durability matters less to the design.
- **W13 reframed, W14 removed.** Earlier W13/W14 rows named solutions
  ("beautiful monospace", "beautiful serif") inside the requirements
  column, conflating _what the user values_ with _which asset delivers it_.
  Replaced with one outcome WHAT (typography sets a writing-tool tone),
  and moved the mono+serif option to §7 as a v1.0 unresolved tension.
  Σ shifted (H9 205→193, H2 198→177, H1 155→148) because the prior
  W13/W14 cells were scoring solution-fit rather than outcome-fit.
- **WHATs swept for solution-shape phrasing.** Following the W13 reframe,
  the same drift was found in W2 (named the key `Ctrl-G`), W4 (named the
  process shape "one-shot"), W7 (named the hardware "surface"), W8 (named
  the medium "e-ink"), W10 (named the deliverable "BOM"), and W9 ("nine
  releases" — brittle vs roadmap reshuffles). All rephrased as outcomes;
  the named solutions remain documented in §7 tradeoffs and the relevant
  ADRs where they belong. Matrix cell strengths held — each cell scored
  the characteristic against the underlying outcome, not the surface
  phrasing — so no Σ recompute.
- **§3 vs §6 priority lists clarified.** The two were giving different
  orderings without saying why. §6 now states explicitly that it is a
  curated rank with two named overrides over §3's pure arithmetic:
  acceptance-criteria critical paths (H4, H5) and table-stakes correctness
  (H8) get manual lifts. §3 now names the HoQ structural bias that makes
  the curation necessary — reward for spread, penalty for narrow-but-
  critical characteristics — using H8/W3 as the canonical example.
- **W14 added — portability outcome.** Captures "I can carry the device
  and write away from a desk" as a distinct WHAT from W11 (multi-day
  battery), weight 8. Recomputed basement Σ; H8 lifted from #6 to #3 in
  the §3 priority list as its voter base widened from W3+W6+W12 to also
  include W14, and H12 entered the top six at #4; H6 dropped out. The
  ID "W14" was previously held by a deprecated typography row (see the
  "W13 reframed, W14 removed" bullet above); the slot is now repurposed.
  §6's "(b) narrow voter base" override for H8 no longer applies and
  has been retired in the §6 preamble.
- **H14 retired — outside §2's scope.** §2 covers measurable engineering
  characteristics — performance metrics of the device's functions, or
  properties of its firmware artifact, memory layout, and build process.
  H14 ("Module count / public-API surface (refactor proxy)") is a
  property of source-code organisation, none of those. The refactor-
  leverage idea survives in §5's component structure and the ADRs that
  decide architectural discipline; it does not need a HoQ matrix slot.
  Removed from §2, the §5 matrix row, the C12 overloaded-list mention,
  and the §4 H14↔H15 conflict bullet. W9's matrix vote shrinks from
  `H10 W + H11 W + H14 S + H15 M` to `H10 W + H11 W + H15 M` — an
  honest reading that "codebase absorbs the planned roadmap" is
  delivered by ADRs, not by a measurable characteristic. ID "H14" left
  as a gap (cross-doc HOW references survive without renumbering H15).
  Total basement Σ drops 1674 → 1557, so rel% recomputed in the §3
  basement.
- **HOWs renamed "characteristics," not "functions."** A function is a
  transformation (input → output); HOWs like H6 "success rate" and
  H10 "binary size" are _measures_ of functions or properties of
  artifacts, not transformations themselves. §2's header, §4's
  ("HOW-vs-HOW tradeoffs"), §5's ("HOW → Component mapping") and
  caption, and §6's column header all cascaded — wherever "function"
  meant HOW. Classical QFD uses "engineering characteristics" (or
  "substitute quality characteristics") for exactly this slot. The
  methodology name in the title (Quality Function Deployment) stays —
  it is the framework's proper noun, not a claim about this doc's
  vocabulary.
- **H6/H7/H8/H12 swept for solution-shape phrasing and measure-vs-
  attribute.** Two drifts in one pass. (a) Solution names inside
  characteristic names: H6 was "`Ctrl-G` push success rate on healthy
  Wi-Fi" — three solutions inside one name (the key, the git verb, the
  transport); H7 was "Push end-to-end (one-file commit)" — git verb and
  its unit; H12 was "Wi-Fi reconnect on transient outage" — transport
  in the name. (b) Measure or behaviour assertion instead of attribute:
  H6's "success rate" is a metric; H8 "Save survives power loss after
  status confirms" is a behaviour assertion. Renamed to pure attributes
  under outcome-shaped conditions: H6 = "Publish reliability (network
  up)", H7 = "Publish latency (one file)", H8 = "Save durability
  (post-confirm power loss)", H12 = "Network reconnect time (transient
  outage)". H7's "latency" pairs with H1's "Type latency".
  Matrix cell strengths held; no Σ recompute.
- **Functions surfaced as their own ontology layer.** Earlier, the
  HOW names packed both a function reference and an attribute
  ("Publish reliability" = Publish [function] + reliability
  [attribute]) without Functions being defined anywhere. §2 now
  opens with a Functions inventory (Type, Save, Publish, Recover,
  Boot, Provision) so the function names HOWs reference have a
  single source of truth. Render and Reconnect remain sub-functions
  referenced inside HOW names; they did not earn top-level slots in
  v0.1. The five-layer ontology stack (WHAT / Function /
  Characteristic / Metric+Unit / Target) is documented in
  [`../GLOSSARY.md`](../GLOSSARY.md), peer to `CONTEXT.md`
  (device vocabulary). With Functions explicit, two arrow-style HOW
  names collapsed for parallelism: H1 "Keypress → glyph latency" →
  "Type latency (keypress → glyph)", H4 "Cold boot → cursor ready" →
  "Boot latency (cold)". The arrow text moved to the parenthetical
  context where it belongs once the function name carries the
  transformation; H4's "to cursor" is implicit in Boot's definition.
  Matrix cell strengths held; no Σ recompute.
- **H4 boot measured; H3 cadence corrected; boot-time docs added
  (2026-07-11).** Cold boot instrumented at **4258 ms** — the ≤ 5 s v0.1 target
  is met; §6's H4 row now carries that measured result and the real mitigation
  (editor rides a full-area partial over the splash, −1.25 s) in place of the
  pre-integration guesses (trim logging / lazy-mount SD). §2's ≤ 3 s v1.0 target
  gained a footnote flagging it **marginal-to-unreachable** — one ~1.9 s full
  refresh is an unavoidable e-ink cold-boot floor. Separately, §2 + §6 H3
  full-refresh cadence corrected from "1 per 20 partials" to **1 per 64**: the
  firmware stretched it (`FULL_REFRESH_EVERY = 64`) once windowed-Y refresh made
  ghosting rare — a drift that predated this pass. New supporting docs:
  [`notes/boot-time-budget.md`](notes/boot-time-budget.md) (waterfall + v1.0
  feasibility) and
  [`tradeoff-curves/epd-refresh-latency.md`](tradeoff-curves/epd-refresh-latency.md)
  (rows-vs-latency model), cross-linked from §4's H1↔H3 bullet.
- **Typoena perception column rebased target → measured (2026-07-11).**
  With v0.1 delivered and hardware-verified, the §3 right-hand zone's Typoena
  profile is the shipped v0.1 result, not a §2 target projection: legend +
  caption relabelled "v0.1 measured", W5 rationale now cites the 4.26 s cold
  boot, W3 notes the verified atomic round-trip (power-pull test still deferred
  to v0.9), **W6 rose 3→4** on the attested 1 h soak, and **W1 dropped 4→2**
  once type latency was measured at ~630 ms (over the revised ≤400 ms target).
  Net Typoena total 52→51, trimming its lead over Pomera to a single point.
  Competitor scores untouched (no new external release). Two drifts caught in the same pass: the
  TikZ W14 row scored Pomera/Smart 2/5 while the authoritative table and totals
  use 5/1 (Smart ~5 lb desk-bound = 1, Pomera pocketable = 5), TikZ corrected
  to match; and the Caveats "thirteen rows" corrected to "fourteen" (§1 has 14
  WHATs).
- **H1 type-latency target relaxed ≤200 → ≤400 ms; v1.0 reset to ≤300 ms
  (2026-07-11).** Cold per-keystroke render measures ~630 ms, so §2's v0.1 H1
  target moved from ≤ 200 ms to ≤ 400 ms and gained a footnote; §3's basement
  target text and §6's rank-4 row followed. The relaxed target is unmet: ~630 ms
  still exceeds ≤ 400 ms (the open v0.1 latency gap), though a longer wait is
  acceptable for now; next-version usage will settle it. The perception W1
  score dropped 4→2 to match. The v1.0 figure was reset from ≤ 150 ms to
  ≤ 300 ms ([ADR-003]'s ~200–300 ms floor); ≤ 150 ms sat below what the panel
  can deliver.

- **This file lagged [ADR-004]'s fired kill-switch by ten days (fixed
  2026-07-16).** Spike 7 fired the kill-switch on 2026-07-06 (gix has no
  HTTPS push; the shipped git engine is `libgit2`/`git2` as an esp-idf CMake
  component), and adr.md recorded it in an "Outcome" section — but this doc
  kept scoring C12 as `gitoxide` and §7 kept "gitoxide over libgit2-sys" as
  the standing decision. §3 narrative, §4 roof bullets, §5 C12 + read-across,
  §6 rank-2 fallback, and the §7 row all rewritten to the libgit2 reality.
  Lesson for the "keep this honest" list: an ADR outcome edit must cascade
  here the same day.
- **`Ctrl-G` → `:gp` swept (2026-07-16).** Publish moved off Ctrl-G to the
  `:gp` ex command (`:sync` → `:gp` rename 2026-07-14); the keymap has no
  Ctrl-G binding at all. W2's rationale and §7's [ADR-010] row updated, and
  [ADR-010] itself was amended the same day with an as-shipped Outcome
  section covering all three of its drifts: the `:gp` trigger, the
  `Typoena publish — unix <epoch>` message (not ISO-8601), and
  replay-not-merge on rejected pushes (the "device may author merge
  commits" consequence never materialised).
- **Config landed on the card, not in encrypted internal flash
  (2026-07-16).** [ADR-005]/[ADR-007] planned "v0.9 moves the secret to
  encrypted LittleFS/NVS with an eFuse key"; v0.9 actually shipped plaintext
  `/sd/typoena.conf` — deliberately, so the wizard and the macOS installer
  produce one identical, desktop-inspectable artifact. C11/C15 are therefore
  still unused, and at-rest protection is the open [ADR-011]. §5's C11
  bullet and §7's auth row updated; the tension is now explicit in §7's
  unresolved list instead of being silently mis-described as done-in-v0.9.
- **H11 stack budget was fiction twice over (2026-07-16).** The ≤ 80 KB
  target priced a five-thread model (usb/wifi/ui/render/git, 76 KB) that no
  longer exists: UI and render run on the main task, Wi-Fi is owned by the
  git thread, and the shipped explicit stacks are git 96 KB + walk 16 KB +
  USB 4+8 KB = **124 KB**. Target revised to ≤ 128 KB (§2 ∥); §6's row now
  carries the measured breakdown. The 96 KB git stack is an [ADR-004]
  consequence the old budget predates.
- **H1's ~630 ms was the wrong tier (2026-07-16).** The 2026-07-11 footnote
  presented ~630 ms as "per-keystroke render", but the refresh-latency curve
  doc shows that figure is the **full-area partial** (deletes, caret moves,
  splash swap); additive typing rides the windowed-Y partial at ~100–130 ms
  (projected — bench confirmation still owed from the on-device refresh
  log). §2 §-footnote rewritten as a two-tier story; perception W1 raised
  2→3, not higher, until the bench number lands and the erase tier gets a
  lever.
- **W15 + H16 added — the companions enter the house (2026-07-16).** The
  product now includes surfaces that are not the device: the macOS installer
  (card provisioner), typoena.dev + `install.sh`, the Typoena GitHub App,
  and the on-device wizard. Their shared user outcome landed as W15 ("a
  first-time user reaches writing without developer tools", weight 7), their
  shared characteristic as H16 (onboarding duration, ≤ 10 min, unmeasured),
  and their parts as C17–C20. Basement Σ recomputed 1557 → 1627 (H12 picks
  up W15's weak vote, 153 → 160); rel% re-derived. The house deliberately
  reads H16 as bottom-tier for the daily writing loop — its weight is about
  product reach, and §6 carries its (unmeasured) budget row.
- **W14's "no enclosure spec yet" was stale (2026-07-16).** The parametric
  OpenSCAD case exists (`hardware/case/`, scad + stl + renders); the score
  stays 2 because portability hinges on [ADR-008]'s battery, not the shell.
  Rationale corrected.
- **[ADR-009] TinyUSB tension retired (2026-07-16).** "If TinyUSB turns out
  unstable, BLE-HID is the fallback" sat in §7's unresolved list since
  before spike 4; the USB host path has since carried every hardware session
  for two weeks. Removed from the live-tension list — reopening it would
  take new evidence, not vigilance.
- **Companion-side doc drift — flagged and then fixed the same day
  (2026-07-16).** The site repo's README called `install.sh` a placeholder
  that "flashes the firmware" — rewritten to the live, checksum-verified,
  never-flashes reality (and its repo pointer corrected to the
  `typoena` org). The installer's `DESIGN.md` still cited
  `installer-v0.1.0` as the release — trued up to the tag-per-release
  model, latest `installer-v0.4.0`; the GitHub release itself was never
  lagging (latest-release already served 0.4.0), only the prose.
  `v0.5-palette-and-multi-file.md`'s header still said "slice 1 of 4" and
  `v0.6-markdown.md`'s still said slice 5 was remaining — both stamped
  **DELIVERED 2026-07-12** to match macroplan and the on-device record.

The earlier variance between README's "~12 lines" and product/[ADR-003]'s
"~11 lines" of "edit area" is now superseded: the side-panel redesign removed
the top header and bottom status bars (metadata moved into the **side panel**),
so the **writing column** spans the full panel height — ~13 lines at the
editor's 20 px font (`FONT_10X20`, `editor.rs` `ROWS = HEIGHT / 20 = 13`).
README, the product/technical docs, and [ADR-003] are all updated to ~13 lines
(writing column).

---

## How to keep this document honest

- When a new ADR lands, add its components to §5 and re-score any
  characteristic-row whose dominant component changed. **The same applies
  when an existing ADR gains an Outcome** (a kill-switch fires, a decision
  reverses): cascade it here the same day — this doc scored the dead
  gitoxide option for ten days after the swap.
- When a spike returns numbers, update §6's "Target" or "Watched on"
  columns — this is the doc that _should_ feel out of date if measured
  reality drifts from estimates.
- The companion surfaces (installer, typoena.dev, GitHub App, wizard) are
  in the house as W15 / H16 / C17–C20 but keep their design records in
  [`../installer/DESIGN.md`](../installer/DESIGN.md) and
  [`v0.9-onboarding-wizard.md`](v0.9-onboarding-wizard.md); when those ship
  changes, re-check those rows rather than re-deriving them here.
- The WHATs (§1) change rarely; the HOWs (§2) change with each release.
  When either changes, re-score the matrix and recompute the basement Σ
  in the §3 diagram; then check the §3 priority list and §4 conflict list
  here still match the new picture.

[ADR-001]: adr.md#adr-001-language-and-runtime--rust-on-esp-idf-rs-std
[ADR-002]: adr.md#adr-002-ui-strategy--custom-widgets-on-embedded-graphics-not-ratatui
[ADR-003]: adr.md#adr-003-display-medium--e-ink-gdey0579t93-panel
[ADR-004]: adr.md#adr-004-git-implementation--gitoxide-gix
[ADR-005]: adr.md#adr-005-auth--https--github-personal-access-token
[ADR-006]: adr.md#adr-006-concurrency--stdthread--channels-no-async-runtime
[ADR-007]: adr.md#adr-007-storage-split--fat-on-sd-for-working-copy-littlefs-on-flash-for-config
[ADR-008]: adr.md#adr-008-mvp-power--wall-powered-battery-deferred-to-v08
[ADR-009]: adr.md#adr-009-keyboard-transport--usb-host-tinyusb
[ADR-010]: adr.md#adr-010-publish-ux--atomic-ctrl-g-auto-timestamp-commit-message-no-user-prompt
[ADR-011]: adr.md#adr-011-credential-provisioning--how-the-pat-reaches-the-device-and-is-protected-at-rest
[ADR-012]: adr.md#adr-012-sd-on-its-own-spi3-host-not-shared-with-the-epd-on-spi2
