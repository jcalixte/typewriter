# House 2 — HOWs × components

_Next house → [Houses 3 & 4](qfd-houses-3-4.md)_

Second house of the QFD cascade (hub: [`qfd.md`](qfd.md)); row
importance carries down from [House 1](qfd-house-1.md)'s basement Σ.

§2's HOWs (rows, importance = each HOW's House-1 basement Σ) × the
components C1–C20. The basement **derives** the component ranking instead
of asserting it: **C5 e-ink panel #1, C7 widget/editor layer #2, C12
libgit2 #3, C2 std runtime #4**: C7's jump past libgit2 is the headline
of the 2026-07-17 W16/H17 re-score (the reach vote lands where the
palette and modal grammar live); C11/C15 sit parenthesised and unranked
(unbuilt). The roof's `−−` between C10 (FAT) and C12 (libgit2) is
Publish's convicted residual; the three `−−` on C6/C7 × C12/C13 are the
July crash record: conflicts mediated by shared memory pools, priced in
[§5's shared-pool budget](#shared-pool-budget--who-allocates-from-what).
Source-of-truth matrix + reading:
[§5](#5-how--component-mapping-phase-2).

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
          \node[anchor=west, font=\scriptsize\bfseries] at (0.55, \qfdLegA)
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

% --- Dimensions tuned for the Phase-2 house (16 HOWs x 20 components) ---
\def\qfdNW{16}
\def\qfdNH{20}
\def\qfdWhatW{4.6}
\def\qfdImpW{0.9}
\def\qfdHdrH{5.0}
\def\qfdBasementN{2}
\qfdshowcompetitivefalse
\qfdshowevallegendfalse

\def\qfdWhatsTitle{Engineering characteristics (H)}
\def\qfdImpTitle{$\Sigma$}

\begin{document}
\begin{qfdhouse}

  % ---------- Rows: the HOWs, carried down from the Phase-1 house ----------
  \pgfmathsetmacro{\qfdWhatTextW}{\qfdWhatW - 0.2}
  \foreach \r/\t in {%
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
    15/{H16 Onboarding duration},
    16/{H17 Reach cost (keystrokes)}%
  }
    \node[anchor=west, font=\scriptsize,
          text width=\qfdWhatTextW cm, align=left]
      at ({\qfdLeftEdge + 0.1}, {-\r + 0.5}) {\t};

  % ---------- Importance: each HOW's Phase-1 basement Sigma ----------
  \foreach \r/\w in {1/178, 2/177, 3/144, 4/62, 5/111, 6/134, 7/27, 8/156, 9/193, 10/41, 11/45, 12/160, 13/137, 14/29, 15/93, 16/117}
    \node[font=\scriptsize] at ({-\qfdImpW/2}, {-\r + 0.5}) {\w};

  % ---------- Columns: components (rotated titles) ----------
  \foreach \c/\t in {%
    1/{C1 ESP32-S3 SoC},
    2/{C2 std runtime (esp-idf)},
    3/{C3 Threads + channels},
    4/{C4 PSRAM allocator},
    5/{C5 E-ink panel GDEY0579T93},
    6/{C6 embedded-graphics + driver},
    7/{C7 Widget / dirty-rect layer},
    8/{C8 Rope buffer (ropey)},
    9/{C9 TinyUSB host},
    10/{C10 FAT on SD (SPI3)},
    11/{C11 LittleFS (unused)},
    12/{C12 libgit2 component},
    13/{C13 mbedTLS},
    14/{C14 GitHub token auth},
    15/{C15 eFuse key (unused)},
    16/{C16 USB-C wall PSU},
    17/{C17 conf + wizard crates},
    18/{C18 macOS installer},
    19/{C19 typoena.dev + install.sh},
    20/{C20 Typoena GitHub App}%
  }
    \node[rotate=90, anchor=west, font=\scriptsize]
      at ({\c - 0.5}, 0.15) {\t};

  % ---------- Relation matrix (S=9, M=3, W=1) — mirrors the table below ----------
  % H1 row 1: C1M C2W C3S C4M C5S C6S C7S C8M C9S
  \node[qfdrel/M] at ({1 - 0.5}, {-1 + 0.5}) {};
  \node[qfdrel/W] at ({2 - 0.5}, {-1 + 0.5}) {};
  \node[qfdrel/S] at ({3 - 0.5}, {-1 + 0.5}) {};
  \node[qfdrel/M] at ({4 - 0.5}, {-1 + 0.5}) {};
  \node[qfdrel/S] at ({5 - 0.5}, {-1 + 0.5}) {};
  \node[qfdrel/S] at ({6 - 0.5}, {-1 + 0.5}) {};
  \node[qfdrel/S] at ({7 - 0.5}, {-1 + 0.5}) {};
  \node[qfdrel/M] at ({8 - 0.5}, {-1 + 0.5}) {};
  \node[qfdrel/S] at ({9 - 0.5}, {-1 + 0.5}) {};
  % H2 row 2: C5S C6S C7S
  \node[qfdrel/S] at ({5 - 0.5}, {-2 + 0.5}) {};
  \node[qfdrel/S] at ({6 - 0.5}, {-2 + 0.5}) {};
  \node[qfdrel/S] at ({7 - 0.5}, {-2 + 0.5}) {};
  % H3 row 3: C5S C6M C7S
  \node[qfdrel/S] at ({5 - 0.5}, {-3 + 0.5}) {};
  \node[qfdrel/M] at ({6 - 0.5}, {-3 + 0.5}) {};
  \node[qfdrel/S] at ({7 - 0.5}, {-3 + 0.5}) {};
  % H4 row 4: C1M C2S C3M C4W C5M C10S C11M
  \node[qfdrel/M] at ({1 - 0.5}, {-4 + 0.5}) {};
  \node[qfdrel/S] at ({2 - 0.5}, {-4 + 0.5}) {};
  \node[qfdrel/M] at ({3 - 0.5}, {-4 + 0.5}) {};
  \node[qfdrel/W] at ({4 - 0.5}, {-4 + 0.5}) {};
  \node[qfdrel/M] at ({5 - 0.5}, {-4 + 0.5}) {};
  \node[qfdrel/S] at ({10 - 0.5}, {-4 + 0.5}) {};
  \node[qfdrel/M] at ({11 - 0.5}, {-4 + 0.5}) {};
  % H5 row 5: C1M C2M C3M C4S C5W C8S C9S C10M C12M C13M
  \node[qfdrel/M] at ({1 - 0.5}, {-5 + 0.5}) {};
  \node[qfdrel/M] at ({2 - 0.5}, {-5 + 0.5}) {};
  \node[qfdrel/M] at ({3 - 0.5}, {-5 + 0.5}) {};
  \node[qfdrel/S] at ({4 - 0.5}, {-5 + 0.5}) {};
  \node[qfdrel/W] at ({5 - 0.5}, {-5 + 0.5}) {};
  \node[qfdrel/S] at ({8 - 0.5}, {-5 + 0.5}) {};
  \node[qfdrel/S] at ({9 - 0.5}, {-5 + 0.5}) {};
  \node[qfdrel/M] at ({10 - 0.5}, {-5 + 0.5}) {};
  \node[qfdrel/M] at ({12 - 0.5}, {-5 + 0.5}) {};
  \node[qfdrel/M] at ({13 - 0.5}, {-5 + 0.5}) {};
  % H6 row 6: C2M C12S C13S C14S
  \node[qfdrel/M] at ({2 - 0.5}, {-6 + 0.5}) {};
  \node[qfdrel/S] at ({12 - 0.5}, {-6 + 0.5}) {};
  \node[qfdrel/S] at ({13 - 0.5}, {-6 + 0.5}) {};
  \node[qfdrel/S] at ({14 - 0.5}, {-6 + 0.5}) {};
  % H7 row 7: C3M C4W C10M C12S C13S
  \node[qfdrel/M] at ({3 - 0.5}, {-7 + 0.5}) {};
  \node[qfdrel/W] at ({4 - 0.5}, {-7 + 0.5}) {};
  \node[qfdrel/M] at ({10 - 0.5}, {-7 + 0.5}) {};
  \node[qfdrel/S] at ({12 - 0.5}, {-7 + 0.5}) {};
  \node[qfdrel/S] at ({13 - 0.5}, {-7 + 0.5}) {};
  % H8 row 8: C2M C10S C11S
  \node[qfdrel/M] at ({2 - 0.5}, {-8 + 0.5}) {};
  \node[qfdrel/S] at ({10 - 0.5}, {-8 + 0.5}) {};
  \node[qfdrel/S] at ({11 - 0.5}, {-8 + 0.5}) {};
  % H9 row 9: C1M C2M C4S C8M C12S C13S
  \node[qfdrel/M] at ({1 - 0.5}, {-9 + 0.5}) {};
  \node[qfdrel/M] at ({2 - 0.5}, {-9 + 0.5}) {};
  \node[qfdrel/S] at ({4 - 0.5}, {-9 + 0.5}) {};
  \node[qfdrel/M] at ({8 - 0.5}, {-9 + 0.5}) {};
  \node[qfdrel/S] at ({12 - 0.5}, {-9 + 0.5}) {};
  \node[qfdrel/S] at ({13 - 0.5}, {-9 + 0.5}) {};
  % H10 row 10: C2S C3W C6M C7M C8M C9M C12S C13M C17W
  \node[qfdrel/S] at ({2 - 0.5}, {-10 + 0.5}) {};
  \node[qfdrel/W] at ({3 - 0.5}, {-10 + 0.5}) {};
  \node[qfdrel/M] at ({6 - 0.5}, {-10 + 0.5}) {};
  \node[qfdrel/M] at ({7 - 0.5}, {-10 + 0.5}) {};
  \node[qfdrel/M] at ({8 - 0.5}, {-10 + 0.5}) {};
  \node[qfdrel/M] at ({9 - 0.5}, {-10 + 0.5}) {};
  \node[qfdrel/S] at ({12 - 0.5}, {-10 + 0.5}) {};
  \node[qfdrel/M] at ({13 - 0.5}, {-10 + 0.5}) {};
  \node[qfdrel/W] at ({17 - 0.5}, {-10 + 0.5}) {};
  % H11 row 11: C3S C9M C12M
  \node[qfdrel/S] at ({3 - 0.5}, {-11 + 0.5}) {};
  \node[qfdrel/M] at ({9 - 0.5}, {-11 + 0.5}) {};
  \node[qfdrel/M] at ({12 - 0.5}, {-11 + 0.5}) {};
  % H12 row 12: C1M C2S C12M C13M
  \node[qfdrel/M] at ({1 - 0.5}, {-12 + 0.5}) {};
  \node[qfdrel/S] at ({2 - 0.5}, {-12 + 0.5}) {};
  \node[qfdrel/M] at ({12 - 0.5}, {-12 + 0.5}) {};
  \node[qfdrel/M] at ({13 - 0.5}, {-12 + 0.5}) {};
  % H13 row 13: C1S C3W C5S C9M C10M C16S
  \node[qfdrel/S] at ({1 - 0.5}, {-13 + 0.5}) {};
  \node[qfdrel/W] at ({3 - 0.5}, {-13 + 0.5}) {};
  \node[qfdrel/S] at ({5 - 0.5}, {-13 + 0.5}) {};
  \node[qfdrel/M] at ({9 - 0.5}, {-13 + 0.5}) {};
  \node[qfdrel/M] at ({10 - 0.5}, {-13 + 0.5}) {};
  \node[qfdrel/S] at ({16 - 0.5}, {-13 + 0.5}) {};
  % H15 row 14: C2S C12S C13M
  \node[qfdrel/S] at ({2 - 0.5}, {-14 + 0.5}) {};
  \node[qfdrel/S] at ({12 - 0.5}, {-14 + 0.5}) {};
  \node[qfdrel/M] at ({13 - 0.5}, {-14 + 0.5}) {};
  % H16 row 15: C10M C12S C13M C14M C17S C18S C19M C20S
  \node[qfdrel/M] at ({10 - 0.5}, {-15 + 0.5}) {};
  \node[qfdrel/S] at ({12 - 0.5}, {-15 + 0.5}) {};
  \node[qfdrel/M] at ({13 - 0.5}, {-15 + 0.5}) {};
  \node[qfdrel/M] at ({14 - 0.5}, {-15 + 0.5}) {};
  \node[qfdrel/S] at ({17 - 0.5}, {-15 + 0.5}) {};
  \node[qfdrel/S] at ({18 - 0.5}, {-15 + 0.5}) {};
  \node[qfdrel/M] at ({19 - 0.5}, {-15 + 0.5}) {};
  \node[qfdrel/S] at ({20 - 0.5}, {-15 + 0.5}) {};
  % H17 row 16: C7S C8M C9M C10M
  \node[qfdrel/S] at ({7 - 0.5}, {-16 + 0.5}) {};
  \node[qfdrel/M] at ({8 - 0.5}, {-16 + 0.5}) {};
  \node[qfdrel/M] at ({9 - 0.5}, {-16 + 0.5}) {};
  \node[qfdrel/M] at ({10 - 0.5}, {-16 + 0.5}) {};

  % ---------- Roof: component-component correlations (documented ones only) ----------
  \node[font=\scriptsize] at (C-2-12)  {$+$};      % std VFS/net is what lets libgit2 run (ADR-001 proved by ADR-004)
  \node[font=\scriptsize] at (C-4-12)  {$+$};      % PSRAM absorbs libgit2 mmap working set (capped)
  \node[font=\scriptsize] at (C-5-7)   {$+\!+$};   % widget dirty-rects aligned to panel regions (ADR-002/003)
  \node[font=\scriptsize] at (C-6-12)  {$-\!-$};   % via DMA reserve: checkout exhausted internal, spi_master NULL-deref (ff :gl crash) — §5 pool budget
  \node[font=\scriptsize] at (C-7-12)  {$-\!-$};   % via PSRAM: push working set starved Frame::new_white, UI thread aborted (run 4) — §5 pool budget
  \node[font=\scriptsize] at (C-7-13)  {$-\!-$};   % via internal DRAM: palette file list vs ssl_setup's 33 KB, TLS refused to start — §5 pool budget
  \node[font=\scriptsize] at (C-10-12) {$-\!-$};   % FAT linear dir scans vs loose objects = the H7 residual
  \node[font=\scriptsize] at (C-12-13) {$+\!+$};   % libgit2 rides ESP-IDF mbedTLS (vendored stream)
  \node[font=\scriptsize] at (C-17-20) {$+\!+$};   % wizard signs in through the App device flow
  \node[font=\scriptsize] at (C-18-19) {$+\!+$};   % install.sh exists to deliver the installer
  \node[font=\scriptsize] at (C-18-20) {$+\!+$};   % installer ^G = the same App device flow

  % ---------- Basement: derived Sigma / rank ----------
  \foreach \c/\abs/\rk in {%
    1/{3345}/{10},
    2/{4588}/{4},
    3/{2785}/{11},
    4/{3359}/{9},
    5/{6021}/{1},
    6/{3750}/{6},
    7/{5667}/{2},
    8/{2586}/{12},
    9/{3621}/{7},
    10/{3417}/{8},
    11/{(1590)}/{--},
    12/{5601}/{3},
    13/{4488}/{5},
    14/{1485}/{13},
    15/{(0)}/{--},
    16/{1233}/{14},
    17/{878}/{15},
    18/{837}/{16},
    19/{279}/{18},
    20/{837}/{16}%
  } {
    \node[font=\scriptsize] at ({\c - 0.5}, {-\qfdNW - 0.5}) {\abs};
    \node[font=\scriptsize\bfseries]
      at ({\c - 0.5}, {-\qfdNW - 1.5}) {\rk};
  }

  % ---------- Basement row labels ----------
  \foreach \k/\lbl in {1/{$\Sigma$ derived}, 2/{Rank}}
    \node[anchor=east, font=\scriptsize\itshape]
      at ({-0.1}, {-\qfdNW - \k + 0.5}) {\lbl};

\end{qfdhouse}
\end{document}
```

---

## 5. HOW → Component mapping (Phase 2)

Which subsystem owns the delivery of each characteristic. Cells are which ADR
constrains the choice.

### The cascade — WHAT → Function → How → Components

The spine of the design, read top-down: which outcomes each Function (§2)
serves, which approach was chosen (with the rejected alternative kept
visible) and which components realise it. The matrices score the same
links exhaustively; this tree is the readable path through them. T-IDs
point at [§7](qfd-tradeoffs.md#7-tradeoffs-and-their-why-linked-to-adrs)'s tradeoff rows.

- **Type** (keypress → glyph + buffer mutated): serves W1 (10), W7 (8), W8 (7), W13 (7) · measured by H1, H2, H3
  - **How: e-ink with windowed partial refresh** ([ADR-003], T3; rejected: FSTN / memory LCD / OLED, no paper aesthetic, no 0 W persistence)
    - C5 GDEY0579T93 panel · C6 `embedded-graphics` + driver
  - **How: custom dirty-rect widget layer** ([ADR-002], T2; rejected: Ratatui, regions don't align to e-ink refresh areas)
    - C7 widget layer · C8 rope buffer
  - **How: USB host keyboard** ([ADR-009], T9; rejected: BLE-HID, radio contention with Wi-Fi during Publish)
    - C9 TinyUSB host · C3 threads
- **Navigate** (intent → active file / buffer / caret repositioned): serves W16 (10), and W2's one-action Publish rides the same grammar · measured by H17, each motion's repaint priced by H1/H2
  - **How: fuzzy file palette over both scopes** (Cmd-P; MRU recents under 2 chars, background card walk interned to one PSRAM blob; rejected: a file-manager screen, a destination that competes with prose)
    - C7 widget layer · C10 FAT walk · C8 rope
  - **How: modal editing grammar** (vim Normal/Visual/ex, counts, `.` repeat, `/` smartcase + accent-folded search; rejected: chorded shortcuts, reach cost grows with document and file count instead of staying O(one motion))
    - C7 · C8 · C9 keyboard
- **Save** (dirty buffer → persisted file on SD): serves W3 (10), W6 (9), W12 (5) · measured by H8
  - **How: atomic-rename + fsync on FAT** ([ADR-007], T7; unlink-first + `*.tmp` boot recovery because FatFS `f_rename` won't overwrite; rejected: LittleFS working copy, a desktop can't read it)
    - C10 FAT on SD (own SPI3, [ADR-012]) · C2 std VFS
- **Publish** (persisted file → commit on remote; sub-function Reconnect): serves W2 (9), W6 (9), W14 (8) · measured by H6, H7, H9, H12
  - **How: libgit2 as an esp-idf CMake component** ([ADR-004] kill-switch outcome, T4; rejected: gitoxide, no HTTPS push)
    - C12 libgit2 + vendored `esp_mbedtls_stream.c` · C13 mbedTLS · C14 token auth
  - **How: splice commit onto the local tip** (T11; rejected: full index write, 611 s / OOM on the real repo)
    - C12 · C10 (dirty-path journal `/sd/.typoena-dirty`)
  - **How: HTTPS + GitHub token** ([ADR-005], T5; rejected: SSH, the device transport doesn't speak it)
    - C13 · C14
  - **How: one atomic `:gp`, auto-timestamp message, replay on rejection** ([ADR-010], T10; rejected: a commit-message prompt, a modal that taxes the writing loop)
    - C12
- **Recover** (degraded file state → readable file): serves W3 (10), W6 (9) · measured by H8, plus H6's replay path
  - **How: journal + boot-time reconciliation** (`*.tmp` recovery, stranded-commit replay, soft-reset reconcile)
    - C10 · C12
- **Boot** (power-on → cursor ready): serves W5 (6), and W15's first impression · measured by H4
  - **How: async splash refresh + background file walk** (2026-07-14 restructure; rejected: synchronous walk, 8.7 s to cursor)
    - C5 · C10 · C2 · C3 (16 KB walk thread)
- **Provision** (uninitialized device → configured device): serves W4 (7), W15 (7) · measured by H16 (its clone leg rides H12)
  - **How: two peer paths to one artifact** (`/sd/typoena.conf` + cloned `/sd/repo`; deliberate redundancy, neither path is a single point of failure for W15)
    - on-device wizard → C17 (`conf` + `wizard` crates)
    - macOS installer → C18, delivered by C19 (typoena.dev `install.sh`, T15, T14)
    - rejected: captive portal (v0.1: ceremony without a user); deferred: SoftAP companion webapp (chosen over BLE 2026-07-16, unbuilt)
  - **How: plaintext conf on the card** (rejected for now: encrypted LittleFS/NVS + eFuse key, the open [ADR-011]; C11/C15 stay unused)
    - C10
  - **How: GitHub App device flow** (rejected as primary: hand-created PAT, kept as the fallback)
    - C20 GitHub App · C14
  - **How: shallow clone + ~30 MB repo gate** (T13; rejected: full clone, exceeds device memory and minutes-scale patience)
    - C12

C1–C4 (SoC, std runtime, threads, PSRAM allocator) underlie every branch:
the platform layer rather than a Function's own component; see "Read
across, not down" below for why C2 is the enabler, not the bottleneck.

### Components and the derived ranking

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
| H12 recon |   3    |   9    |        |        |        |       |        |         |        |        |         |    3    |    3    |         |         |         |         |          |          |         |
| H13 mA    |   9    |        |   1    |        |   9    |       |        |         |   3    |   3    |         |         |         |         |         |    9    |         |          |          |         |
| H15 build |        |   9    |        |        |        |       |        |         |        |        |         |    9    |    3    |         |         |         |         |          |          |         |
| H16 onb   |        |        |        |        |        |       |        |         |        |   3    |         |    9    |    3    |    3    |         |         |    9    |    9     |    3     |    9    |
| H17 reach |        |        |        |        |        |       |   9    |    3    |   3    |   3    |         |         |         |         |         |         |         |          |          |         |
| **Σ**     |  3345  |  4588  |  2785  |  3359  |  6021  | 3750  |  5667  |  2586   |  3621  |  3417  | (1590)  |  5601   |  4488   |  1485   |   (0)   |  1233   |   878   |   837    |   279    |   837   |
| **Rank**  |   10   |   4    |   11   |   9    |   1    |   6   |   2    |   12    |   7    |   8    |    —    |    3    |    5    |   13    |    —    |   14    |   15    |   16     |   18     |   16    |

The **Σ row carries the cascade down**: component Σ = Σ(basement Σ of each
HOW × cell strength), so component priorities are derived from the house,
not asserted. C11 and C15 are parenthesised and excluded from the rank:
they are unbuilt ([ADR-007]'s possible future shape / the open [ADR-011]),
so their votes are fiction until they ship; C15's 0 is the sanity check
(no shipped characteristic touches it). C18/C20 tie at 837. Recompute this
row whenever the basement Σ or a cell above changes. H17's strong cell
lands on C7, the widget layer read broadly as the editor surface: the
palette, the modal grammar's rendering, and the dirty-rects that price
each motion all live in the `editor` crate behind it.

This matrix is drawn as **House 2** at the top of this page
(row importance = each HOW's Phase-1 basement Σ, the derived Σ / Rank as
basement, the documented component correlations in the roof; C11/C15
parenthesised and unranked). **This markdown matrix is the source of
truth**: re-score here first, then mirror the drawing, same day.

### Read across, not down

- **C5/C7/C6** (panel + widget/editor + graphics) are the most leveraged
  cluster (15 438 summed Σ, ~27 % of all component votes) and the
  2026-07-17 W16 re-score made the cluster's internal order the headline:
  **C7 jumped #5 to #2** (5 667), overtaking libgit2. H17's strong vote
  lands where the palette and the modal grammar live, so the derived
  ranking now points at the same place the July effort record did: the
  divergence D1 used as evidence is dissolved by arithmetic, not excused.
  C5 stays #1 (6 021); C6 holds #6. [ADR-002]
  and [ADR-003] are the ADRs to keep most honest as v0.x progresses.
- **C12** (`libgit2`) is **#3 by derivation** (5 601, was #2) and overloaded:
  H6, H7, H9, H10, H11, H12, H15 (and now
  H16's clone leg) all touch it. [ADR-004]'s kill-switch **fired**
  (spike 7, 2026-07-06: gix had no HTTPS push) and the fallback became the
  component: `git2` vendored as an esp-idf CMake component with two
  deliberate C-side deltas (`esp_mbedtls_stream.c`: a double-free fix in the
  error path reported upstream, plus TLS session resumption). The overload
  prediction held: the real-repo push campaign (2026-07-13) was fought
  entirely inside C12's memory profile. [ADR-010] pins the _shape_ of the
  publish sequence; swapping the library under it never changed the user
  contract, exactly as designed.
- **C11** (LittleFS) is **still unused, and v0.9 decided against it for
  config**: the wizard/installer write plaintext `/sd/typoena.conf` on the
  card (one artifact both provisioning paths share, desktop-inspectable).
  C15 (eFuse key) is likewise unused; token-at-rest protection is the open
  [ADR-011]. C11's non-zero cells describe a possible future shape per
  [ADR-007], not shipped reality, which is why its Σ (1 590, a would-be
  #13, above actually-shipped C14) is parenthesised and unranked. The
  derivation caught this: score fictional cells and they outrank real
  components.
- **C17–C20** (wizard, installer, site, GitHub App) form the onboarding
  cluster that owns H16. Deliberate redundancy: C17 and C18 are peer paths
  to the same configured card, so neither is a single point of failure for
  W15: a Mac user never touches the wizard; a computer-less user never
  touches the installer. C20 (device-flow auth) and C12 (shallow clone) are
  the shared spine of both. **Rank-vs-effort flag, resolved 2026-07-17:**
  these still rank #15–#18, but the divergence the flag pointed at is
  gone. [`house-vs-product.md`](house-vs-product.md) D1 read the July
  effort record as revealed preference that the WHAT weights were stale,
  and the W16/H17 re-score confirmed it: the votes moved to where the
  effort went (C7 #2; C8, C9, C10 all rose), while C17–C20 stay low for
  the honest reason: onboarding is once per device. The flag's early
  trigger (a second firing) retires with D1.
- **C2** (std runtime) is **#4 by derivation** (4 588): it sits
  underneath almost everything, but it's the
  _enabler_ (H4 boot, H10 binary, H12 reconnect) rather than the bottleneck.
  Reversing [ADR-001] would force re-deciding [ADR-004], [ADR-005],
  [ADR-006], [ADR-007] all at once: they're a single decision in three
  drawers.
- **The roof reads sparse because the real coupling is pool-mediated.**
  At the call level the components are genuinely decoupled, and some of
  that emptiness was *bought*, not found ([ADR-012] deleted the SD↔EPD
  bus conflict, the 96 KB git thread keeps libgit2 off the UI, the editor
  core is pure and host-tested). What remains runs through three shared
  memory pools instead of the call graph, and is priced in the
  [shared-pool budget](#shared-pool-budget--who-allocates-from-what)
  below: the source of truth for the three pool-mediated `−−` roof
  cells (C6↔C12, C7↔C12, C7↔C13).

### Shared-pool budget — who allocates from what

The House-2 roof was first scored from the call graph (who talks to
whom) and by that reading stayed nearly empty. Every crash of the July
push campaign came through a channel the call graph can't see: **shared
memory pools**. Three crashes, three roof cells the first scoring
missed, all `−−`, all mediated:

- **C7 ↔ C12 via PSRAM**: the push's mmap working set exhausted PSRAM
  and `Frame::new_white`'s 26 KB draw allocation died on `HalfPageUp`,
  OOM-aborting the UI thread mid-push (run 4). Fixes: persistent
  two-frame draw (repaints never allocate) + mwindow/odb caps.
- **C7 ↔ C13 via internal DRAM**: the palette's 1 098 path
  `Vec<String>`s held ~60–70 KB of internal DRAM and `ssl_setup`'s
  ~33 KB internal-only allocation failed, so TLS refused to start.
  Fixes: `CONFIG_MBEDTLS_EXTERNAL_MEM_ALLOC=y` + the file list interned
  to one PSRAM blob.
- **C6 ↔ C12 via the DMA reserve**: ff `:gl`'s checkout exhausted
  internal DRAM, a DMA-capable allocation inside `spi_master` returned
  NULL and the driver dereferenced it (repo safe, device down). Fix:
  reserve doubled 32 to 64 KB.

A pairwise roof structurally underprices these: pool contention is
N-way (every consumer conflicts with every other consumer through the
pool) and fragmenting that into C(N,2) glyphs is the House-2 sibling of
the blindness that fragmented flow across House 1's rows
([`house-vs-product.md`](house-vs-product.md) D1). Making a pool a
House-2 *column* was considered and rejected: matrix columns rank where
the next unit of effort goes, and a pool is not a place effort can go:
every memory-flavoured HOW would vote it to #1 and distort the cascade
into Houses 3–4. The shape that fits is the transpose: **consumers ×
pools**, cells = worst-observed draw, column arithmetic = the crash
condition.

| Consumer | Internal DRAM (512 KB SRAM) | PSRAM (8 MB octal) | DMA-capable reserve (64 KB, carved from internal) |
| --- | --- | --- | --- |
| C3 threads | stacks 124 KB: git 96 + walk 16 + USB 4+8 (≤ 128 KB budget, [§6](qfd-budget.md#6-critical-performance-budget)) | — | — |
| C5/C6 display | — | 2 × 26 KB persistent frames (allocated once at boot, the run-4 lesson) | every EPD SPI transfer |
| C7 widget / palette | *(was 182 KB of file list, interned away 2026-07-14)* | 51 KB path blob | — |
| C8 rope buffer | — | open file, capped 256 KB | — |
| C10 FAT on SD | FatFS work areas + 16-FD mount (unmeasured) | — | every SD SPI transfer |
| C12 libgit2 | 96 KB thread stack (counted under C3) | mwindow ≤ 1.5 MB + odb cache ≤ 1 MB + pack `.idx` map ~1.7 MB (whole-file, outside the mwindow budget) | — |
| C13 mbedTLS | *(was ~33 KB `ssl_setup`, moved 2026-07-13 via `EXTERNAL_MEM_ALLOC`)* | TLS session ~35 KB | — |
| C1/C2 system (Wi-Fi, lwIP, esp-idf) | the standing floor, unmeasured, the pool's biggest unpriced tenant | — | — |
| **Worst observed (min-ever free)** | **2 099 B** (run 9, mid-TLS-send) | **684 B** pre-cap (run 6); **4.5 MB** post-cap (run 9) | alloc failure at the old 32 KB (the ff `:gl` crash) |

Reading it: each of the three crashes is one column touching zero, and
internal DRAM's proven margin at its worst moment is **~2 KB**. The
bottom row's telemetry already exists (`log_push_heap` prints per-pool
free, min-evers, and the largest PSRAM block; `esp_map` live-bytes
tracks the mmap share): when a new consumer lands or a min-ever moves,
update this table first, then draw (or retire) the roof cell it
justifies. The reusable lesson: on a 512 KB-internal SoC the roof's real
axis is **"who allocates from what," not "who calls whom"**: the call
graph predicted none of the three crashes; this table's column sums
would have flagged all of them.

### Houses 3–4

The cascade continues into process and controls on
[`qfd-houses-3-4.md`](qfd-houses-3-4.md): House 3 rows carry this
page's derived component Σ down as row importance.


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
