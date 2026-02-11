import { ButtonHTMLAttributes, forwardRef } from 'react'
import { motion } from 'framer-motion'

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: 'primary' | 'secondary' | 'ghost'
  size?: 'sm' | 'md' | 'lg'
}

const Button = forwardRef<HTMLButtonElement, ButtonProps>(
  ({ children, variant = 'primary', size = 'md', className = '', disabled, onClick, type = 'button' }, ref) => {
    const baseStyles = `
      inline-flex items-center justify-center font-medium tracking-wide uppercase
      transition-all duration-200 ease-out
      focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--text-secondary)] focus-visible:ring-offset-2 focus-visible:ring-offset-[var(--bg-primary)]
      disabled:opacity-50 disabled:cursor-not-allowed
    `

    const variants = {
      primary: `
        bg-white text-black
        hover:bg-gray-100
        active:scale-[0.98]
      `,
      secondary: `
        bg-transparent text-white border border-[var(--border-default)]
        hover:border-[var(--text-secondary)]
        active:scale-[0.98]
      `,
      ghost: `
        bg-transparent text-[var(--text-secondary)]
        hover:text-white
      `,
    }

    const sizes = {
      sm: 'px-3 py-1.5 text-xs rounded-sm',
      md: 'px-6 py-3 text-sm rounded-sm',
      lg: 'px-8 py-4 text-base rounded-sm',
    }

    return (
      <motion.button
        ref={ref}
        type={type}
        whileTap={{ scale: disabled ? 1 : 0.98 }}
        className={`${baseStyles} ${variants[variant]} ${sizes[size]} ${className}`}
        disabled={disabled}
        onClick={onClick}
      >
        {children}
      </motion.button>
    )
  }
)

Button.displayName = 'Button'

export default Button
