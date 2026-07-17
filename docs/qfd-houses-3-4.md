# Houses 3 & 4 — processes × controls (pipeline reading)

_Next house → [Quality House (empty template)](quality-house-empty.md)_

Third and fourth houses of the QFD cascade (hub: [`qfd.md`](qfd.md)),
drawn under a deliberate reinterpretation: a solo-built device has no
factory, so "process" means the toolchain + release pipeline (P1–P9,
firmware build through GitHub-App administration) and "production
controls" means the verification practices (Q1–Q8, host tests through
the end-to-end install-chain check). The literal manufacturing reading
would be scaffolding; the pipeline reading is where this project's real
production risk lives. Row importance carries down from
[House 2](qfd-house-2.md)'s derived component Σ.

## House 3 — components × processes (pipeline reading)

Components (rows, importance = the derived House-2 Σ) × the processes
that produce them. No factory: "process" is the toolchain + release
pipeline P1–P9. **P1 firmware build carries 52.4 % of the process weight;
P4 bench assembly is #2 (21.4 %) with only manual controls**; the
CS-jumper and SDXC lessons were both paid there. Catalogue + first-cut
caveat: [the narrative below](#houses-34--the-cascade-to-process-and-controls).

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

% --- Dimensions tuned for House 3 (20 components x 9 processes) ---
\def\qfdNW{20}
\def\qfdNH{9}
\def\qfdWhatW{4.6}
\def\qfdImpW{1.2}
\def\qfdHdrH{5.0}
\def\qfdBasementN{2}
\qfdshowcompetitivefalse
\qfdshowevallegendfalse
\def\qfdImpTitle{$\Sigma$}

\def\qfdWhatsTitle{Components (C)}

\begin{document}
\begin{qfdhouse}

  \pgfmathsetmacro{\qfdWhatTextW}{\qfdWhatW - 0.2}
  \foreach \r/\t in {%
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
    \node[anchor=west, font=\scriptsize,
          text width=\qfdWhatTextW cm, align=left]
      at ({\qfdLeftEdge + 0.1}, {-\r + 0.5}) {\t};

  \foreach \r/\w in {1/{3345}, 2/{4588}, 3/{2785}, 4/{3359}, 5/{6021}, 6/{3750}, 7/{5667}, 8/{2586}, 9/{3621}, 10/{3417}, 11/{(1590)}, 12/{5601}, 13/{4488}, 14/{1485}, 15/{(0)}, 16/{1233}, 17/{878}, 18/{837}, 19/{279}, 20/{837}}
    \node[font=\scriptsize] at ({-\qfdImpW/2}, {-\r + 0.5}) {\w};

  \foreach \c/\t in {%
    1/{P1 Firmware build (cargo + esp-idf)},
    2/{P2 libgit2 CMake component build},
    3/{P3 Flash at manufacturing},
    4/{P4 Bench hardware assembly},
    5/{P5 Provision card -- wizard},
    6/{P6 Provision card -- installer},
    7/{P7 Installer release cut (tag + sha)},
    8/{P8 Site deploy (Coolify)},
    9/{P9 GitHub App / org admin}%
  }
    \node[rotate=90, anchor=west, font=\scriptsize]
      at ({\c - 0.5}, 0.15) {\t};

  % ---------- Relation matrix (S=9, M=3, W=1) — which process step creates or materially shapes each shipped component ----------
  % C1 row 1: P3M P4S
  \node[qfdrel/M] at ({3 - 0.5}, {-1 + 0.5}) {};
  \node[qfdrel/S] at ({4 - 0.5}, {-1 + 0.5}) {};
  % C2 row 2: P1S P3M
  \node[qfdrel/S] at ({1 - 0.5}, {-2 + 0.5}) {};
  \node[qfdrel/M] at ({3 - 0.5}, {-2 + 0.5}) {};
  % C3 row 3: P1S
  \node[qfdrel/S] at ({1 - 0.5}, {-3 + 0.5}) {};
  % C4 row 4: P1S
  \node[qfdrel/S] at ({1 - 0.5}, {-4 + 0.5}) {};
  % C5 row 5: P1M P4S
  \node[qfdrel/M] at ({1 - 0.5}, {-5 + 0.5}) {};
  \node[qfdrel/S] at ({4 - 0.5}, {-5 + 0.5}) {};
  % C6 row 6: P1S
  \node[qfdrel/S] at ({1 - 0.5}, {-6 + 0.5}) {};
  % C7 row 7: P1S
  \node[qfdrel/S] at ({1 - 0.5}, {-7 + 0.5}) {};
  % C8 row 8: P1S
  \node[qfdrel/S] at ({1 - 0.5}, {-8 + 0.5}) {};
  % C9 row 9: P1S P4M
  \node[qfdrel/S] at ({1 - 0.5}, {-9 + 0.5}) {};
  \node[qfdrel/M] at ({4 - 0.5}, {-9 + 0.5}) {};
  % C10 row 10: P1M P4S P5M P6M
  \node[qfdrel/M] at ({1 - 0.5}, {-10 + 0.5}) {};
  \node[qfdrel/S] at ({4 - 0.5}, {-10 + 0.5}) {};
  \node[qfdrel/M] at ({5 - 0.5}, {-10 + 0.5}) {};
  \node[qfdrel/M] at ({6 - 0.5}, {-10 + 0.5}) {};
  % C12 row 12: P1M P2S
  \node[qfdrel/M] at ({1 - 0.5}, {-12 + 0.5}) {};
  \node[qfdrel/S] at ({2 - 0.5}, {-12 + 0.5}) {};
  % C13 row 13: P1S P2M
  \node[qfdrel/S] at ({1 - 0.5}, {-13 + 0.5}) {};
  \node[qfdrel/M] at ({2 - 0.5}, {-13 + 0.5}) {};
  % C14 row 14: P1M P5S P6S P9M
  \node[qfdrel/M] at ({1 - 0.5}, {-14 + 0.5}) {};
  \node[qfdrel/S] at ({5 - 0.5}, {-14 + 0.5}) {};
  \node[qfdrel/S] at ({6 - 0.5}, {-14 + 0.5}) {};
  \node[qfdrel/M] at ({9 - 0.5}, {-14 + 0.5}) {};
  % C16 row 16: P4S
  \node[qfdrel/S] at ({4 - 0.5}, {-16 + 0.5}) {};
  % C17 row 17: P1S P5M
  \node[qfdrel/S] at ({1 - 0.5}, {-17 + 0.5}) {};
  \node[qfdrel/M] at ({5 - 0.5}, {-17 + 0.5}) {};
  % C18 row 18: P6M P7S
  \node[qfdrel/M] at ({6 - 0.5}, {-18 + 0.5}) {};
  \node[qfdrel/S] at ({7 - 0.5}, {-18 + 0.5}) {};
  % C19 row 19: P7M P8S
  \node[qfdrel/M] at ({7 - 0.5}, {-19 + 0.5}) {};
  \node[qfdrel/S] at ({8 - 0.5}, {-19 + 0.5}) {};
  % C20 row 20: P5M P6M P9S
  \node[qfdrel/M] at ({5 - 0.5}, {-20 + 0.5}) {};
  \node[qfdrel/M] at ({6 - 0.5}, {-20 + 0.5}) {};
  \node[qfdrel/S] at ({9 - 0.5}, {-20 + 0.5}) {};

  % ---------- Roof ----------
  \node[font=\scriptsize] at (C-1-2) {$+\!+$};   % cargo build drives the CMake component build
  \node[font=\scriptsize] at (C-5-6) {$+\!+$};   % peer provisioning paths, one artifact
  \node[font=\scriptsize] at (C-7-8) {$+$};   % release + site serve the same one-liner

  % ---------- Basement: relative weight / rank ----------
  \foreach \c/\rel/\rk in {%
    1/{52.4}/{1},
    2/{10.0}/{3},
    3/{3.7}/{6},
    4/{21.4}/{2},
    5/{4.5}/{4},
    6/{4.5}/{5},
    7/{1.3}/{8},
    8/{0.4}/{9},
    9/{1.9}/{7}%
  } {
    \node[font=\scriptsize] at ({\c - 0.5}, {-\qfdNW - 0.5}) {\rel};
    \node[font=\scriptsize\bfseries]
      at ({\c - 0.5}, {-\qfdNW - 1.5}) {\rk};
  }
  \foreach \k/\lbl in {1/{Rel.\ \%}, 2/{Rank}}
    \node[anchor=east, font=\scriptsize\itshape]
      at ({-0.1}, {-\qfdNW - \k + 0.5}) {\lbl};

\end{qfdhouse}
\end{document}
```

## House 4 — processes × controls

Processes (rows, importance = House-3 rel-%) × the verification
practices that guard them, Q1–Q8. **Q2 on-device verification #1, Q3
build gates #2**: the hardware-verify-everything habit is where the
arithmetic says control effort belongs. Q6's checksum chain ranks #8 by
breadth while being the *sole* control on the public install path.
Reading: [the narrative below](#houses-34--the-cascade-to-process-and-controls).

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

% --- Dimensions tuned for House 4 (9 processes x 8 controls) ---
\def\qfdNW{9}
\def\qfdNH{8}
\def\qfdWhatW{4.6}
\def\qfdImpW{0.9}
\def\qfdHdrH{5.0}
\def\qfdBasementN{2}
\qfdshowcompetitivefalse
\qfdshowevallegendfalse
\def\qfdImpTitle{\%}

\def\qfdWhatsTitle{Processes (P)}

\begin{document}
\begin{qfdhouse}

  \pgfmathsetmacro{\qfdWhatTextW}{\qfdWhatW - 0.2}
  \foreach \r/\t in {%
    1/{P1 Firmware build (cargo + esp-idf)},
    2/{P2 libgit2 CMake component build},
    3/{P3 Flash at manufacturing},
    4/{P4 Bench hardware assembly},
    5/{P5 Provision card -- wizard},
    6/{P6 Provision card -- installer},
    7/{P7 Installer release cut (tag + sha)},
    8/{P8 Site deploy (Coolify)},
    9/{P9 GitHub App / org admin}%
  }
    \node[anchor=west, font=\scriptsize,
          text width=\qfdWhatTextW cm, align=left]
      at ({\qfdLeftEdge + 0.1}, {-\r + 0.5}) {\t};

  % Row importance = House 3's basement (P rel-%). A prior revision
  % mistakenly carried the Q basement values here (8 entries for 9 rows);
  % caught in the 2026-07-17 re-derivation — see qfd-changelog.md.
  \foreach \r/\w in {1/{52.4}, 2/{10.0}, 3/{3.7}, 4/{21.4}, 5/{4.5}, 6/{4.5}, 7/{1.3}, 8/{0.4}, 9/{1.9}}
    \node[font=\scriptsize] at ({-\qfdImpW/2}, {-\r + 0.5}) {\w};

  \foreach \c/\t in {%
    1/{Q1 Host test suites (cargo test)},
    2/{Q2 On-device verification runs},
    3/{Q3 Build gates (just build / -light)},
    4/{Q4 Bench instrumentation + telemetry},
    5/{Q5 Card safety guards},
    6/{Q6 Checksum + quarantine chain},
    7/{Q7 Acceptance: soak / boot / power-pull},
    8/{Q8 End-to-end install-chain check}%
  }
    \node[rotate=90, anchor=west, font=\scriptsize]
      at ({\c - 0.5}, 0.15) {\t};

  % ---------- Relation matrix (S=9, M=3, W=1) — which control guards each process step ----------
  % P1 row 1: Q1S Q2S Q3S Q4M Q7M
  \node[qfdrel/S] at ({1 - 0.5}, {-1 + 0.5}) {};
  \node[qfdrel/S] at ({2 - 0.5}, {-1 + 0.5}) {};
  \node[qfdrel/S] at ({3 - 0.5}, {-1 + 0.5}) {};
  \node[qfdrel/M] at ({4 - 0.5}, {-1 + 0.5}) {};
  \node[qfdrel/M] at ({7 - 0.5}, {-1 + 0.5}) {};
  % P2 row 2: Q2S Q3S Q4M
  \node[qfdrel/S] at ({2 - 0.5}, {-2 + 0.5}) {};
  \node[qfdrel/S] at ({3 - 0.5}, {-2 + 0.5}) {};
  \node[qfdrel/M] at ({4 - 0.5}, {-2 + 0.5}) {};
  % P3 row 3: Q2M Q7M
  \node[qfdrel/M] at ({2 - 0.5}, {-3 + 0.5}) {};
  \node[qfdrel/M] at ({7 - 0.5}, {-3 + 0.5}) {};
  % P4 row 4: Q2S Q4M Q7M
  \node[qfdrel/S] at ({2 - 0.5}, {-4 + 0.5}) {};
  \node[qfdrel/M] at ({4 - 0.5}, {-4 + 0.5}) {};
  \node[qfdrel/M] at ({7 - 0.5}, {-4 + 0.5}) {};
  % P5 row 5: Q1M Q2S Q5S Q8M
  \node[qfdrel/M] at ({1 - 0.5}, {-5 + 0.5}) {};
  \node[qfdrel/S] at ({2 - 0.5}, {-5 + 0.5}) {};
  \node[qfdrel/S] at ({5 - 0.5}, {-5 + 0.5}) {};
  \node[qfdrel/M] at ({8 - 0.5}, {-5 + 0.5}) {};
  % P6 row 6: Q5S Q8M
  \node[qfdrel/S] at ({5 - 0.5}, {-6 + 0.5}) {};
  \node[qfdrel/M] at ({8 - 0.5}, {-6 + 0.5}) {};
  % P7 row 7: Q6S Q8S
  \node[qfdrel/S] at ({6 - 0.5}, {-7 + 0.5}) {};
  \node[qfdrel/S] at ({8 - 0.5}, {-7 + 0.5}) {};
  % P8 row 8: Q6M Q8S
  \node[qfdrel/M] at ({6 - 0.5}, {-8 + 0.5}) {};
  \node[qfdrel/S] at ({8 - 0.5}, {-8 + 0.5}) {};
  % P9 row 9: Q8S
  \node[qfdrel/S] at ({8 - 0.5}, {-9 + 0.5}) {};

  % ---------- Roof ----------
  \node[font=\scriptsize] at (C-2-4) {$+$};   % device runs generate the telemetry the bench reads
  \node[font=\scriptsize] at (C-2-7) {$+$};   % on-device runs feed the acceptance evidence

  % ---------- Basement: relative weight / rank ----------
  \foreach \c/\rel/\rk in {%
    1/{19.5}/{3},
    2/{32.4}/{1},
    3/{22.6}/{2},
    4/{10.1}/{4},
    5/{3.3}/{6},
    6/{0.5}/{8},
    7/{9.3}/{5},
    8/{2.4}/{7}%
  } {
    \node[font=\scriptsize] at ({\c - 0.5}, {-\qfdNW - 0.5}) {\rel};
    \node[font=\scriptsize\bfseries]
      at ({\c - 0.5}, {-\qfdNW - 1.5}) {\rk};
  }
  \foreach \k/\lbl in {1/{Rel.\ \%}, 2/{Rank}}
    \node[anchor=east, font=\scriptsize\itshape]
      at ({-0.1}, {-\qfdNW - \k + 0.5}) {\lbl};

\end{qfdhouse}
\end{document}
```

---

## Houses 3–4 — the cascade to process and controls

Classical QFD carries the cascade two houses further: components deploy
into the **process** that produces them (House 3), and the process deploys
into the **controls** that keep it honest (House 4). This project has no
factory, but it does have a production system: the toolchain and release
pipeline (P1–P9) and the verification practices that guard it (Q1–Q8).
Both houses (drawn at the top of this page) are scored under that reading. **First cut, scored
2026-07-16**: the P/Q catalogues and cells are asserted from the
documented pipeline (justfile, installer DESIGN, release chain, the
hardware-verification record), single-rater, not measured: re-score when
the pipeline changes shape.

Row importance carries down the cascade as in [House 2](qfd-house-2.md): House 3 rows carry
each component's derived Σ (C11/C15 parenthesised and excluded, as
everywhere), House 4 rows carry each process's House-3 relative weight.
Basements show relative weight + rank (raw Σ grows geometrically down the
cascade and stops being readable).

**The processes**: P1 firmware build (`just build`: cargo for xtensa +
ESP-IDF), P2 the libgit2 esp-idf CMake component build (vendored deltas;
needs its own fingerprint handling), P3 flash at manufacturing (devices
ship pre-flashed, [installer DESIGN](../installer/DESIGN.md)), P4 bench
hardware assembly (panel, SPI3 SD, PSU, case), P5/P6 the two peer card
provisioning paths (wizard / installer), P7 the installer release cut
(`installer-v*` tag → universal binary + `.sha256`), P8 site deploy
(Coolify auto-deploy), P9 GitHub App + org administration (client_id,
scopes, token-expiry policy, the one "process" no repo builds).

The scored house is drawn as **House 3** at the top of this page.

**The controls**: Q1 host test suites (editor 237 / keymap 29 / wizard 39),
Q2 on-device verification runs (the hardware-verified stamps throughout
this file), Q3 build gates (`just build` / `build-light`), Q4 bench
instrumentation + telemetry (`sd_bench`, refresh log, `log_push_heap`,
boot timestamps), Q5 card safety guards (ambiguity refusal, dirty-guard,
`dot_clean`, token-never-derived), Q6 the checksum + quarantine chain on
the public install path, Q7 acceptance tests (1 h soak, cold-boot clock,
the owed power-pull), Q8 the end-to-end install-chain check (mirror →
release → typoena.dev, device-flow e2e).

The scored house is drawn as **House 4** at the top of this page.

**Reading the pair.** P1 carries **52.4 %** of the process weight: the
firmware build produces almost every high-Σ component, which is why Q2/Q3
(on-device verification + build gates) rank #1/#2 among controls: the
project's habit of hardware-verifying every slice is exactly where the
arithmetic says the control effort belongs. Two flags worth keeping:
**P4 bench assembly is #2 (21.4 %) with only manual controls**: nothing
automated guards the wiring that C5/C10/C16 depend on (the CS-jumper and
SDXC lessons were both paid here), so hardware changes deserve the same
verify-on-device discipline as code; and **Q6's rank #8 understates it**:
the checksum chain is the *only* control on the public install path, so
its breadth-based rank reads low exactly the way H8's once did in House 1
(narrow voter base, absolute stakes for its one voter).


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
