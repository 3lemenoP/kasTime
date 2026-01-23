import { ButtonHTMLAttributes, forwardRef } from 'react'
import { motion } from 'framer-motion'

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: 'primary' | 'secondary' | 'ghost'
  size?: 'sm' | 'md' | 'lg'
}

const Button = forwardRef<HTMLButtonElement, ButtonProps>(
  ({ children, variant = 'primary', size = 'md', className = '', disabled, ...props }, ref) => {
    const baseStyles = `
      inline-flex items-center justify-center font-medium tracking-wide uppercase
      transition-all duration-200 ease-out
      focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent-primary)] focus-visible:ring-offset-2 focus-visible:ring-offset-[var(--bg-primary)]
      disabled:opacity-50 disabled:cursor-not-allowed
    `

    const variants = {
      primary: `
        bg-[var(--accent-primary)] text-[var(--text-inverse)]
        hover:brightness-110 hover:shadow-[var(--glow-active)]
        active:scale-[0.98]
      `,
      secondary: `
        bg-transparent text-[var(--text-primary)] border border-[var(--border-default)]
        hover:border-[var(--accent-primary)] hover:text-[var(--accent-primary)]
        active:scale-[0.98]
      `,
      ghost: `
        bg-transparent text-[var(--accent-secondary)]
        hover:underline
      `,
    }

    const sizes = {
      sm: 'px-3 py-1.5 text-xs rounded',
      md: 'px-6 py-3 text-sm rounded',
      lg: 'px-8 py-4 text-base rounded-md',
    }

    return (
      <motion.button
        ref={ref}
        whileTap={{ scale: disabled ? 1 : 0.98 }}
        className={`${baseStyles} ${variants[variant]} ${sizes[size]} ${className}`}
        disabled={disabled}
        {...props}
      >
        {children}
      </motion.button>
    )
  }
)

Button.displayName = 'Button'

export default Button
