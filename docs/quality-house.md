# Quality House

The 13 WHATs × 15 HOWs House of Quality. The roof carries function-vs-
function correlations; the basement carries v0.1 targets (mirrored from
[`qfd.md`](qfd.md) §2) plus the weighted-vote sums
(`Σ = Σ(W weight × cell strength)`) and rounded relative weights. The
narrative reading of these numbers lives in `qfd.md` §3 (priority list)
and §4 (conflict list).

This file is the authoritative source for the matrix cells, the roof
correlations, and the basement Σ + relative weights. WHATs (§1) and HOWs
(§2) are mirrored here from `qfd.md`; if those diverge, trust `qfd.md`.

Out of scope here: §5 component mapping (would need a second house),
§6 critical performance budget (already a curated rank), §7 tradeoffs
(narrative), §8 inconsistencies (history).

The perception zone scores four products against the WHATs on a 0–5 scale.
**These are educated guesses, not measurements** — see the
[scoring rationale](#perception-scores-guessed) below for what each cell
is based on. Useful for self-positioning ("where do we land vs the
market"), not as a fair head-to-head buyer's guide.

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
\def\qfdAltOneLabel{Our product}
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

% --- Dimensions tuned for the typewriter QFD (13 W x 15 H) ---
\def\qfdNW{13}
\def\qfdNH{15}
\def\qfdWhatW{4.6}
\def\qfdImpW{0.7}
\def\qfdHdrH{4.2}
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
\definecolor{qfdcOurs}{RGB}{0,114,178}
\definecolor{qfdcRem}{RGB}{213,94,0}
\definecolor{qfdcFrw}{RGB}{0,158,115}
\definecolor{qfdcPom}{RGB}{204,121,167}

\tikzset{
  qfdalt1mk/.style={circle, draw=qfdcOurs, fill=qfdcOurs!55!white,
                    minimum size=6.5pt, inner sep=0pt, line width=1.1pt},
  qfdalt1ln/.style={line width=1.2pt, qfdcOurs},
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
}

\begin{document}
\begin{qfdhouse}

  % ---------- WHATs (left column) ----------
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
    13/{W13 Typography sets a writing-tool tone}%
  }
    \node[anchor=west, font=\scriptsize,
          text width={\qfdWhatW - 0.2}cm, align=left]
      at ({\qfdLeftEdge + 0.1}, {-\r + 0.5}) {\t};

  % ---------- Importance (raw 1-10 weight) ----------
  \foreach \r/\w in {1/10, 2/9, 3/10, 4/7, 5/6, 6/9, 7/8, 8/7,
                     9/8, 10/5, 11/4, 12/5, 13/7}
    \node[font=\scriptsize] at ({-\qfdImpW/2}, {-\r + 0.5}) {\w};

  % ---------- HOWs (rotated column titles) ----------
  \foreach \c/\t in {%
    1/{H1 Keypress$\to$glyph latency},
    2/{H2 Refresh area per keystroke},
    3/{H3 Full-refresh cadence},
    4/{H4 Cold boot to cursor},
    5/{H5 Continuous-typing endurance},
    6/{H6 Push success rate},
    7/{H7 Push end-to-end time},
    8/{H8 Save durability vs power loss},
    9/{H9 PSRAM heap headroom},
    10/{H10 Firmware binary size},
    11/{H11 Total stack budget},
    12/{H12 Wi-Fi reconnect time},
    13/{H13 Idle / typing / push current},
    14/{H14 Module / API surface count},
    15/{H15 Clean release build time}%
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

  % W4 row 4: H6M H12M H14W
  \node[qfdrel/M] at ({6 - 0.5},  {-4 + 0.5}) {};
  \node[qfdrel/M] at ({12 - 0.5}, {-4 + 0.5}) {};
  \node[qfdrel/W] at ({14 - 0.5}, {-4 + 0.5}) {};

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

  % W7 row 7: H1M H2M H3M H13M H14W
  \node[qfdrel/M] at ({1 - 0.5},  {-7 + 0.5}) {};
  \node[qfdrel/M] at ({2 - 0.5},  {-7 + 0.5}) {};
  \node[qfdrel/M] at ({3 - 0.5},  {-7 + 0.5}) {};
  \node[qfdrel/M] at ({13 - 0.5}, {-7 + 0.5}) {};
  \node[qfdrel/W] at ({14 - 0.5}, {-7 + 0.5}) {};

  % W8 row 8: H1W H2S H3S
  \node[qfdrel/W] at ({1 - 0.5},  {-8 + 0.5}) {};
  \node[qfdrel/S] at ({2 - 0.5},  {-8 + 0.5}) {};
  \node[qfdrel/S] at ({3 - 0.5},  {-8 + 0.5}) {};

  % W9 row 9: H10W H11W H14S H15M
  \node[qfdrel/W] at ({10 - 0.5}, {-9 + 0.5}) {};
  \node[qfdrel/W] at ({11 - 0.5}, {-9 + 0.5}) {};
  \node[qfdrel/S] at ({14 - 0.5}, {-9 + 0.5}) {};
  \node[qfdrel/M] at ({15 - 0.5}, {-9 + 0.5}) {};

  % W10 row 10: H10M H13W H14M H15W
  \node[qfdrel/M] at ({10 - 0.5}, {-10 + 0.5}) {};
  \node[qfdrel/W] at ({13 - 0.5}, {-10 + 0.5}) {};
  \node[qfdrel/M] at ({14 - 0.5}, {-10 + 0.5}) {};
  \node[qfdrel/W] at ({15 - 0.5}, {-10 + 0.5}) {};

  % W11 row 11: H13S
  \node[qfdrel/S] at ({13 - 0.5}, {-11 + 0.5}) {};

  % W12 row 12: H6W H8M H14M
  \node[qfdrel/W] at ({6 - 0.5},  {-12 + 0.5}) {};
  \node[qfdrel/M] at ({8 - 0.5},  {-12 + 0.5}) {};
  \node[qfdrel/M] at ({14 - 0.5}, {-12 + 0.5}) {};

  % W13 row 13: H9M
  \node[qfdrel/M] at ({9 - 0.5},  {-13 + 0.5}) {};

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
  \node[font=\scriptsize] at (C-10-15) {$-\!-$};   % H10-H15 binary vs build
  \node[font=\scriptsize] at (C-11-13) {$-$};      % H11-H13
  \node[font=\scriptsize] at (C-14-15) {$-$};      % H14-H15 modularity vs build

  % ---------- Basement: target / abs weight / rel weight % ----------
  \foreach \c/\tgt/\abs/\rel in {%
    1/{$\leq$200\,ms}/148/10,
    2/{$\leq$1 line}/177/11,
    3/{1 : 20}/144/9,
    4/{$\leq$5\,s}/54/3,
    5/{$\geq$1\,h}/111/7,
    6/{$\geq$95\,\%}/134/9,
    7/{$\leq$30\,s}/27/2,
    8/{100\,\%}/132/9,
    9/{$\geq$1\,MB}/193/12,
    10/{$\leq$2\,MB}/41/3,
    11/{$\leq$80\,KB}/45/3,
    12/{$\leq$30\,s}/129/8,
    13/{obs.}/65/4,
    14/{$\leq$8}/117/8,
    15/{$\leq$7\,min}/29/2%
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

  % ---------- Perception zone: 4 products x 13 WHATs (0-5 scores) ----------
  % Columns: \so=Ours target, \st=reMarkable 2 + Type Folio,
  %          \sf=Freewrite Traveler, \sg=Pomera DM250.
  % Pass 1: stash each score as a named coordinate so the profile lines
  % below can reuse it without recomputing.
  \foreach \r/\so/\st/\sf/\sg in {%
    1/4/3/3/5,
    2/5/4/4/2,
    3/4/4/2/2,
    4/5/2/2/5,
    5/4/3/3/5,
    6/3/3/4/5,
    7/5/2/5/5,
    8/4/3/4/5,
    9/4/3/1/1,
    10/5/4/2/1,
    11/1/5/5/4,
    12/3/1/2/3,
    13/3/5/2/2%
  } {
    \pgfmathsetmacro{\xo}{\qfdNH + (\so + 0.5)*\qfdCmpW/6}
    \pgfmathsetmacro{\xt}{\qfdNH + (\st + 0.5)*\qfdCmpW/6}
    \pgfmathsetmacro{\xf}{\qfdNH + (\sf + 0.5)*\qfdCmpW/6}
    \pgfmathsetmacro{\xg}{\qfdNH + (\sg + 0.5)*\qfdCmpW/6}
    \coordinate (po-\r) at (\xo, {-\r + 0.5});
    \coordinate (pr-\r) at (\xt, {-\r + 0.5});
    \coordinate (pf-\r) at (\xf, {-\r + 0.5});
    \coordinate (pp-\r) at (\xg, {-\r + 0.5});
  }

  % Pass 2: profile lines per alternative. Drawn before markers so the
  % dots sit on top of (not under) the line endpoints.
  \draw[qfdalt1ln] (po-1) \foreach \r in {2,...,\qfdNW} { -- (po-\r) };
  \draw[qfdalt2ln] (pr-1) \foreach \r in {2,...,\qfdNW} { -- (pr-\r) };
  \draw[qfdalt3ln] (pf-1) \foreach \r in {2,...,\qfdNW} { -- (pf-\r) };
  \draw[qfdalt4ln] (pp-1) \foreach \r in {2,...,\qfdNW} { -- (pp-\r) };

  % Pass 3: markers on top of the lines.
  \foreach \r in {1,...,\qfdNW} {
    \node[qfdalt1mk] at (po-\r) {};
    \node[qfdalt2mk] at (pr-\r) {};
    \node[qfdalt3mk] at (pf-\r) {};
    \node[qfdalt4mk] at (pp-\r) {};
  }

  % ---------- Manual legend (4 alternatives, placed right of zones) ----------
  \pgfmathsetmacro{\qfdLegX}{\qfdNH + \qfdCmpW + 0.7}
  \begin{scope}[shift={(\qfdLegX, \qfdHdrH - 0.4)}]
    \draw[qfdmed, rounded corners=2pt]
      (-0.15, 0.4) rectangle (5.1, -7.05);
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
        {Ours (v0.1 target)};
    \draw[qfdalt2ln] (0.05, -5.25) -- (0.45, -5.25);
      \node[qfdalt2mk] at (0.25, -5.25) {};
      \node[anchor=west] at (0.55, -5.25) {reMarkable 2 + Type Folio};
    \draw[qfdalt3ln] (0.05, -5.70) -- (0.45, -5.70);
      \node[qfdalt3mk] at (0.25, -5.70) {};
      \node[anchor=west] at (0.55, -5.70) {Freewrite Traveler};
    \draw[qfdalt4ln] (0.05, -6.15) -- (0.45, -6.15);
      \node[qfdalt4mk] at (0.25, -6.15) {};
      \node[anchor=west] at (0.55, -6.15) {Pomera DM250};
    \node[anchor=west, font=\scriptsize\itshape] at (0, -6.70)
      {0 = poor, 5 = excellent};
  \end{scope}

\end{qfdhouse}
\end{document}
```

## Perception scores (guessed)

Four products on the 0–5 scale, scored against each WHAT. Reference
configurations: **reMarkable 2 + Type Folio**, **Freewrite Traveler**,
**Pomera DM250** (DM250 has a reflective monochrome LCD, not e-ink — flagged
in W1 / W8). "Ours" is the v0.1 target from `qfd.md` §2, not measured yet.

| ID  | WHAT (truncated)                                | Ours | reM. | Frw. | Pom. | Rationale (shortest defensible)                                                                                  |
| --- | ----------------------------------------------- | :--: | :--: | :--: | :--: | ---------------------------------------------------------------------------------------------------------------- |
| W1  | Sub-second response to typing                   |  4   |  3   |  3   |  5   | Ours targets ≤200 ms; reMarkable + Freewrite (e-ink) are visibly laggy; Pomera's reflective LCD is near-zero.    |
| W2  | Publishing is one deliberate action away        |  5   |  4   |  4   |  2   | Ctrl-G atomic; reMarkable + Freewrite cloud-sync is one-tap but not git; Pomera = USB/SD copy or QR transfer.    |
| W3  | Pulling power never corrupts the file           |  4   |  4   |  2   |  2   | Ours: atomic-rename + fsync. reMarkable journals. Freewrite + Pomera: forum reports of corruption on yank.       |
| W4  | Provisioning never interrupts writing           |  5   |  2   |  2   |  5   | Ours v0.1: build-time config (dev-only). reM/Frw need Wi-Fi + account. Pomera: literally none.                   |
| W5  | Quick boot to a writing cursor                  |  4   |  3   |  3   |  5   | Ours target ≤5 s. reMarkable cold-boots ~20 s (great from sleep). Pomera ~3 s.                                   |
| W6  | Long sessions without crash / lag / drift       |  3   |  3   |  4   |  5   | Ours unproven (1 h target). Freewrite famously stable. Pomera firmware is decades-mature.                        |
| W7  | Nothing on the device competes with prose       |  5   |  2   |  5   |  5   | reMarkable has apps, menus, drawing, PDFs. Freewrite + Pomera are single-purpose; ours by design.                |
| W8  | The UI never moves except when I move it        |  4   |  3   |  4   |  5   | reMarkable animates more; ours uses dirty-rects; Freewrite minimal motion; Pomera near-static LCD.               |
| W9  | Codebase absorbs the planned roadmap            |  4   |  3   |  1   |  1   | Modular Rust ours; reMarkable is hackable Linux; Freewrite + Pomera are closed firmware.                         |
| W10 | I can repair or fork it with hobbyist tools     |  5   |  4   |  2   |  1   | Ours: open BOM + ESP32. reMarkable: rooted Linux + community ROMs. Freewrite + Pomera: closed.                   |
| W11 | Multi-day battery life (v0.8 onward)            |  1   |  5   |  5   |  4   | Ours v0.1 = wall-powered (battery deferred). reMarkable + Freewrite Traveler legendary. Pomera ~24 h.            |
| W12 | Local-only files coexist with git scope         |  3   |  1   |  2   |  3   | Ours v0.5+ design. reMarkable cloud-only. Freewrite has local + Postbox but no VCS. Pomera = pure local.         |
| W13 | Typography sets a writing-tool tone             |  3   |  5   |  2   |  2   | Ours v0.1: single mono (serif option in v1.0). reMarkable: rich type rendering. Freewrite + Pomera: utilitarian. |

**Totals** (sum across 13 WHATs, no weighting): Ours 50, Pomera 45,
reMarkable 42, Freewrite 39. Pomera ranking second is the useful signal —
we're effectively building "Pomera + Wi-Fi + git + hackable BOM", which
re-confirms the differentiation thesis from `README.md`.

Weighted totals (Σ score × W weight) tell the same story with more
contrast — left as exercise; the unweighted view is enough to read the
picture.

### Caveats

- **Single-rater bias.** All thirteen rows are scored from the project
  author's POV. A reMarkable buyer would weight W11 (battery) at 10 and
  W12 (git) at 1, flipping the totals.
- **Configuration matters.** Freewrite Smart Typewriter (e-ink) scores
  differently from Traveler (LCD) on W1 / W5. The table picks Traveler
  as the most direct competitor.
- **W3 / W6 Freewrite scores are anecdotal.** Forum reports, not bench
  data. Treat the 2 / 4 as "we'd need to test this" rather than fact.
- **No price column.** Ours-as-BOM is materially cheaper than the
  competitors but cost is not a WHAT in `qfd.md` §1, so it's absent here.
  Worth a row if a v0.x WHAT ever calls it out.

## Reading the house

- **Importance (left column)** is the raw 1–10 weight from `qfd.md` §1, not
  a normalised %, so adding stays cheap when a WHAT shifts. Sum of weights
  is 95; treat each unit as ~1.05 % if you want a percentage view.
- **Roof** carries the §4 symbols translated into classical QFD glyphs:
  `++` strong reinforcement (`◎`), `+` mild reinforcement (`○`), `−` mild
  conflict (`×`), `−−` strong conflict (`⊗`).
- **Basement rows** are: v0.1 target → §3 column sum (`Σ` of
  `weight × strength`) → relative weight as integer % of total (1546).
  Relative weights round to 100.
- **H7, H10, H15** (push time, binary size, build time) sit at the bottom
  of the basement — knowingly-paid costs per `qfd.md` §7, not signals to
  optimise harder.

## Regenerating

The matrix cells (`\node[qfdrel/{S,M,W}]`), roof symbols (`C-i-j` slots),
and basement Σ + Rel% are native to this file — re-score directly in the
TikZ source, then update the §3 priority list and §4 conflict list in
`qfd.md` to match the new picture.

When `qfd.md` §1 or §2 changes:

1. Importance column → §1 weight column.
2. HOW titles + v0.1 targets → §2 target column.
3. Recompute basement Σ for any HOW whose column changed: per-cell
   contribution = `(W weight) × (cell strength: 9 / 3 / 1 / 0)`.
4. Recompute relative weight: each Σ ÷ total Σ × 100, rounded to integer
   percent.

Perception scores are **not** derived from `qfd.md` — they live only in
this file. Update them when (a) a competitor ships a relevant change,
(b) measurement replaces a guess, or (c) a WHAT is added/removed in §1.
Each score keeps its one-line rationale in the table above.

If a renderer rejects the `tikz` fence, the file is still readable as
source — the placement comments name each WHAT, HOW, and cell. The
perception-scores table above is the human-readable fallback for the
right-hand zone of the diagram.
