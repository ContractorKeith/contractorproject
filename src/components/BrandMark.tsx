interface BrandMarkProps {
  size?: number;
}

export function BrandMark({ size = 44 }: BrandMarkProps) {
  return (
    <span className="brand-mark" style={{ width: size, height: size }} aria-hidden="true">
      <svg viewBox="0 0 32 32">
        <rect x="2" y="4" width="16" height="6" className="brand-mark__paper" />
        <rect x="8" y="13" width="20" height="6" className="brand-mark__accent" />
        <rect x="4" y="22" width="12" height="6" className="brand-mark__paper" />
        <rect x="16.4" y="10" width="1.6" height="3" className="brand-mark__accent" />
        <rect x="8" y="19" width="1.6" height="3" className="brand-mark__accent" />
      </svg>
    </span>
  );
}
