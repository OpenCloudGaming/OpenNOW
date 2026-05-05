import type { JSX } from "react";

interface ButtonProps {
  className?: string;
  size?: number;
}

export function ButtonA({ className, size = 18 }: ButtonProps): JSX.Element {
  return (
    <svg 
      width={size} 
      height={size} 
      viewBox="0 0 24 24" 
      className={className}
      aria-hidden="true"
      fill="none" 
      xmlns="http://www.w3.org/2000/svg"
    >
      <circle cx="12" cy="12" r="10" stroke="#58d98a" strokeWidth="2.5" fill="rgba(88, 217, 138, 0.1)" />
      <text x="50%" y="50%" dominantBaseline="central" textAnchor="middle" fontSize="12" fontWeight="900" fill="#58d98a" fontFamily="Inter, system-ui">A</text>
    </svg>
  );
}

export function ButtonB({ className, size = 18 }: ButtonProps): JSX.Element {
  return (
    <svg 
      width={size} 
      height={size} 
      viewBox="0 0 24 24" 
      className={className}
      aria-hidden="true"
      fill="none" 
      xmlns="http://www.w3.org/2000/svg"
    >
      <circle cx="12" cy="12" r="10" stroke="#ef4444" strokeWidth="2.5" fill="rgba(239, 68, 68, 0.1)" />
      <text x="50%" y="50%" dominantBaseline="central" textAnchor="middle" fontSize="12" fontWeight="900" fill="#ef4444" fontFamily="Inter, system-ui">B</text>
    </svg>
  );
}

export function ButtonX({ className, size = 18 }: ButtonProps): JSX.Element {
  return (
    <svg 
      width={size} 
      height={size} 
      viewBox="0 0 24 24" 
      className={className}
      aria-hidden="true"
      fill="none" 
      xmlns="http://www.w3.org/2000/svg"
    >
      <circle cx="12" cy="12" r="10" stroke="#3b82f6" strokeWidth="2.5" fill="rgba(59, 130, 246, 0.1)" />
      <text x="50%" y="50%" dominantBaseline="central" textAnchor="middle" fontSize="12" fontWeight="900" fill="#3b82f6" fontFamily="Inter, system-ui">X</text>
    </svg>
  );
}

export function ButtonY({ className, size = 18 }: ButtonProps): JSX.Element {
  return (
    <svg 
      width={size} 
      height={size} 
      viewBox="0 0 24 24" 
      className={className}
      aria-hidden="true"
      fill="none" 
      xmlns="http://www.w3.org/2000/svg"
    >
      <circle cx="12" cy="12" r="10" stroke="#eab308" strokeWidth="2.5" fill="rgba(234, 179, 8, 0.1)" />
      <text x="50%" y="50%" dominantBaseline="central" textAnchor="middle" fontSize="12" fontWeight="900" fill="#eab308" fontFamily="Inter, system-ui">Y</text>
    </svg>
  );
}

// PlayStation-style icons
export function ButtonPSCross({ className, size = 18 }: ButtonProps): JSX.Element {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" className={className} aria-hidden="true" fill="none" xmlns="http://www.w3.org/2000/svg">
      <circle cx="12" cy="12" r="10" stroke="#3b82f6" strokeWidth="2" fill="rgba(59,130,246,0.06)" />
      <path d="M8 8 L16 16 M16 8 L8 16" stroke="#3b82f6" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

export function ButtonPSCircle({ className, size = 18 }: ButtonProps): JSX.Element {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" className={className} aria-hidden="true" fill="none" xmlns="http://www.w3.org/2000/svg">
      <circle cx="12" cy="12" r="10" stroke="#ef4444" strokeWidth="2" fill="rgba(239,68,68,0.06)" />
      <circle cx="12" cy="12" r="4.5" stroke="#ef4444" strokeWidth="1.6" fill="none" />
    </svg>
  );
}

export function ButtonPSSquare({ className, size = 18 }: ButtonProps): JSX.Element {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" className={className} aria-hidden="true" fill="none" xmlns="http://www.w3.org/2000/svg">
      <circle cx="12" cy="12" r="10" stroke="#a78bfa" strokeWidth="2" fill="rgba(167,139,250,0.06)" />
      <rect x="8" y="8" width="8" height="8" rx="1" stroke="#a78bfa" strokeWidth="1.6" fill="none" />
    </svg>
  );
}

export function ButtonPSTriangle({ className, size = 18 }: ButtonProps): JSX.Element {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" className={className} aria-hidden="true" fill="none" xmlns="http://www.w3.org/2000/svg">
      <circle cx="12" cy="12" r="10" stroke="#22c55e" strokeWidth="2" fill="rgba(34,197,94,0.06)" />
      <polygon points="12,7 16.5,16 7.5,16" stroke="#22c55e" strokeWidth="1.6" fill="none" strokeLinejoin="round" strokeLinecap="round" />
    </svg>
  );
}
