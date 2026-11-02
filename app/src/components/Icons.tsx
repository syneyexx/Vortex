// Lijn-iconen (inline SVG, 1.6px lijn) in de stijl van de referentie.
import type { ReactElement, SVGProps } from "react";

type P = SVGProps<SVGSVGElement> & { size?: number };
const base = (size = 20, rest: SVGProps<SVGSVGElement> = {}) => ({
  width: size,
  height: size,
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.6,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
  ...rest,
});

const mk = (d: ReactElement) =>
  function Icon({ size, ...rest }: P) {
    return <svg {...base(size, rest)}>{d}</svg>;
  };

export const IcDoc = mk(
  <>
    <path d="M7 3h7l4 4v14H7z" />
    <path d="M14 3v4h4M10 12h5M10 15h5M10 18h3" />
  </>,
);
export const IcNodes = mk(
  <>
    <circle cx="12" cy="5" r="2.2" />
    <circle cx="5" cy="18" r="2.2" />
    <circle cx="19" cy="18" r="2.2" />
    <path d="M11 7l-5 9M13 7l5 9M7 18h10" />
  </>,
);
export const IcSearch = mk(
  <>
    <circle cx="11" cy="11" r="6.5" />
    <path d="M16 16l5 5" />
  </>,
);
export const IcScale = mk(
  <>
    <path d="M12 4v16M7 20h10M5 7h14" />
    <path d="M5 7l-3 6a3 3 0 006 0zM19 7l-3 6a3 3 0 006 0z" />
  </>,
);
export const IcTarget = mk(
  <>
    <circle cx="12" cy="12" r="8" />
    <circle cx="12" cy="12" r="4" />
    <path d="M12 12l7-7M16 5h3v3" />
  </>,
);
export const IcGlobe = mk(
  <>
    <circle cx="12" cy="12" r="9" />
    <path d="M3 12h18M12 3c3 3.5 3 14.5 0 18M12 3c-3 3.5-3 14.5 0 18" />
  </>,
);
export const IcFile = mk(
  <>
    <path d="M7 3h7l4 4v14H7z" />
    <path d="M14 3v4h4M10 12h5M10 16h5" />
  </>,
);
export const IcCode = mk(<path d="M9 7l-5 5 5 5M15 7l5 5-5 5M13 5l-2 14" />);
export const IcCheck = mk(<path d="M5 12.5l4.5 4.5L19 7.5" />);
export const IcCheckCircle = mk(
  <>
    <circle cx="12" cy="12" r="9" />
    <path d="M8 12.5l3 3 5.5-6" />
  </>,
);
export const IcUser = mk(
  <>
    <circle cx="12" cy="8" r="4" />
    <path d="M4 21c1.5-4 4.5-6 8-6s6.5 2 8 6" />
  </>,
);
export const IcClock = mk(
  <>
    <circle cx="12" cy="12" r="9" />
    <path d="M12 7v5l3 2" />
  </>,
);
export const IcPlus = mk(<path d="M12 5v14M5 12h14" />);
export const IcBolt = mk(<path d="M13 3L5 13h6l-1 8 8-10h-6z" />);
export const IcLayers = mk(
  <>
    <path d="M12 3l9 4.5-9 4.5-9-4.5z" />
    <path d="M3 12l9 4.5 9-4.5M3 16.5L12 21l9-4.5" />
  </>,
);
export const IcSliders = mk(<path d="M4 7h10M18 7h2M4 17h2M10 17h10M14 4v6M6 14v6" />);
export const IcChevron = mk(<path d="M6 9l6 6 6-6" />);
export const IcChevronRight = mk(<path d="M9 6l6 6-6 6" />);
export const IcPause = mk(<path d="M9 6v12M15 6v12" />);
export const IcBook = mk(
  <>
    <path d="M3 5c3-1 6-1 9 1 3-2 6-2 9-1v14c-3-1-6-1-9 1-3-2-6-2-9-1z" />
    <path d="M12 6v14" />
  </>,
);
export const IcMonitor = mk(
  <>
    <rect x="3" y="4" width="18" height="12" rx="1.5" />
    <path d="M2 20h20" />
  </>,
);
export const IcGear = mk(
  <>
    <circle cx="12" cy="12" r="3" />
    <path d="M12 2v3M12 19v3M4.2 4.2l2.1 2.1M17.7 17.7l2.1 2.1M2 12h3M19 12h3M4.2 19.8l2.1-2.1M17.7 6.3l2.1-2.1" />
  </>,
);
export const IcChip = mk(
  <>
    <rect x="6" y="6" width="12" height="12" rx="2" />
    <rect x="9.5" y="9.5" width="5" height="5" />
    <path d="M9 2v4M15 2v4M9 18v4M15 18v4M2 9h4M2 15h4M18 9h4M18 15h4" />
  </>,
);
export const IcDb = mk(
  <>
    <ellipse cx="12" cy="5" rx="7" ry="2.5" />
    <path d="M5 5v14c0 1.4 3.1 2.5 7 2.5s7-1.1 7-2.5V5M5 12c0 1.4 3.1 2.5 7 2.5s7-1.1 7-2.5" />
  </>,
);
export const IcModel = mk(
  <>
    <circle cx="12" cy="12" r="9" />
    <path d="M8 12a4 4 0 018 0M12 3v4M12 17v4" />
  </>,
);
export const IcFork = mk(
  <>
    <circle cx="6" cy="5" r="2" />
    <circle cx="18" cy="5" r="2" />
    <circle cx="12" cy="19" r="2" />
    <path d="M6 7v2a3 3 0 003 3h6a3 3 0 003-3V7M12 12v5" />
  </>,
);
export const IcCopy = mk(
  <>
    <rect x="8" y="8" width="12" height="12" rx="2" />
    <path d="M16 8V5a1 1 0 00-1-1H5a1 1 0 00-1 1v10a1 1 0 001 1h3" />
  </>,
);
export const IcStop = mk(<rect x="7" y="7" width="10" height="10" rx="1.5" />);
