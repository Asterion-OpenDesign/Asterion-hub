export function Star({ size = 22 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" aria-hidden fill="currentColor">
      <path d="M12 0.8 14.1 9.9 23.2 12 14.1 14.1 12 23.2 9.9 14.1 0.8 12 9.9 9.9Z" />
    </svg>
  )
}

export function Wordmark() {
  return (
    <div className="wordmark">
      <span className="wordmark__mark">
        <Star size={20} />
      </span>
      <span className="wordmark__text">
        <strong>ASTERION</strong>
        <span>Hub</span>
      </span>
    </div>
  )
}
