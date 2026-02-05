import { motion } from 'framer-motion'

interface ThermodynamicGaugeProps {
  blueWork: string
  btcEquivalent: string
  isAnimated?: boolean
  size?: 'sm' | 'md' | 'lg'
}

const sizeConfig = {
  sm: { height: 'h-4', text: 'text-xs', valueText: 'text-sm' },
  md: { height: 'h-6', text: 'text-sm', valueText: 'text-base' },
  lg: { height: 'h-8', text: 'text-base', valueText: 'text-lg' },
}

function ThermodynamicGauge({
  blueWork,
  btcEquivalent,
  isAnimated = true,
  size = 'md',
}: ThermodynamicGaugeProps): JSX.Element {
  const { height, text, valueText } = sizeConfig[size]

  return (
    <div className="w-full max-w-lg mx-auto">
      {/* Gauge bar container */}
      <div className={`relative ${height} bg-[var(--bg-tertiary)] rounded-full overflow-hidden border border-[var(--border-subtle)]`}>
        {/* Gradient fill bar */}
        <motion.div
          className="absolute inset-y-0 left-0 rounded-full"
          style={{
            background: 'linear-gradient(90deg, var(--thermo-cold) 0%, var(--thermo-warm) 50%, var(--thermo-hot) 100%)',
            width: '100%',
            transformOrigin: 'left',
          }}
          initial={{ scaleX: 0 }}
          animate={{ scaleX: isAnimated ? 1 : 0 }}
          transition={{
            duration: 1.2,
            ease: [0.16, 1, 0.3, 1],
            delay: 0.2,
          }}
        />

        {/* Glow overlay on the hot end */}
        <motion.div
          className="absolute inset-y-0 right-0 w-1/3 rounded-r-full"
          style={{
            background: 'linear-gradient(90deg, transparent 0%, rgba(236, 72, 153, 0.3) 100%)',
          }}
          initial={{ opacity: 0 }}
          animate={{ opacity: isAnimated ? 1 : 0 }}
          transition={{ duration: 0.5, delay: 1.2 }}
        />

        {/* Shine effect */}
        <div
          className="absolute inset-0 rounded-full"
          style={{
            background: 'linear-gradient(180deg, rgba(255,255,255,0.1) 0%, transparent 50%, rgba(0,0,0,0.1) 100%)',
          }}
        />
      </div>

      {/* Labels */}
      <div className="flex justify-between items-center mt-3">
        <div className="flex items-baseline gap-2">
          <span className={`font-data ${valueText} font-semibold text-[var(--accent-primary)]`}>
            {blueWork}
          </span>
          <span className={`${text} text-[var(--text-tertiary)]`}>blue work</span>
        </div>
        <span className={`${text} text-[var(--text-tertiary)]`}>
          ~{btcEquivalent} BTC confirmations
        </span>
      </div>

      {/* Temperature scale indicators */}
      <div className="flex justify-between mt-2 px-1">
        <span className="text-micro text-[var(--thermo-cold)]">COLD</span>
        <span className="text-micro text-[var(--thermo-warm)]">WARM</span>
        <span className="text-micro text-[var(--thermo-hot)]">HOT</span>
      </div>
    </div>
  )
}

export default ThermodynamicGauge
