/** @type {import('tailwindcss').Config} */
export default {
  darkMode: ["class"],
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  theme: {
    extend: {
      colors: {
        border: "hsl(var(--border))",
        input: "hsl(var(--input))",
        ring: "hsl(var(--ring))",
        background: "hsl(var(--background))",
        foreground: "hsl(var(--foreground))",
        primary: {
          DEFAULT: "hsl(var(--primary))",
          foreground: "hsl(var(--primary-foreground))",
        },
        secondary: {
          DEFAULT: "hsl(var(--secondary))",
          foreground: "hsl(var(--secondary-foreground))",
        },
        destructive: {
          DEFAULT: "hsl(var(--destructive))",
          foreground: "hsl(var(--destructive-foreground))",
        },
        muted: {
          DEFAULT: "hsl(var(--muted))",
          foreground: "hsl(var(--muted-foreground))",
        },
        accent: {
          DEFAULT: "hsl(var(--accent))",
          foreground: "hsl(var(--accent-foreground))",
        },
        popover: {
          DEFAULT: "hsl(var(--popover))",
          foreground: "hsl(var(--popover-foreground))",
        },
        card: {
          DEFAULT: "hsl(var(--card))",
          foreground: "hsl(var(--card-foreground))",
        },
        success: {
          DEFAULT: "hsl(var(--success))",
          foreground: "hsl(var(--success-foreground))",
        },
        warning: {
          DEFAULT: "hsl(var(--warning))",
          foreground: "hsl(var(--warning-foreground))",
        },
        info: {
          DEFAULT: "hsl(var(--info))",
          foreground: "hsl(var(--info-foreground))",
        },
        status: {
          recording: "hsl(var(--status-recording))",
          connected: "hsl(var(--status-connected))",
          disconnected: "hsl(var(--status-disconnected))",
          idle: "hsl(var(--status-idle))",
        },
        "accent-pro": {
          DEFAULT: "hsl(var(--accent-pro))",
          hover: "hsl(var(--accent-pro-hover))",
          foreground: "hsl(var(--accent-pro-foreground))",
        },
        gaming: {
          cyan: "hsl(var(--gaming-cyan))",
          magenta: "hsl(var(--gaming-magenta))",
          purple: "hsl(var(--gaming-purple))",
          sidebar: "hsl(var(--gaming-sidebar))",
        },
      },
      borderRadius: {
        lg: "var(--radius)",
        md: "calc(var(--radius) - 2px)",
        sm: "calc(var(--radius) - 4px)",
      },
      keyframes: {
        shimmer: {
          "0%": { backgroundPosition: "200% 0" },
          "100%": { backgroundPosition: "-200% 0" },
        },
        "pulse-red": {
          "0%": { boxShadow: "0 0 0 0 rgba(255, 0, 60, 0.7)" },
          "70%": { boxShadow: "0 0 0 10px rgba(255, 0, 60, 0)" },
          "100%": { boxShadow: "0 0 0 0 rgba(255, 0, 60, 0)" },
        },
        "pulse-cyan": {
          "0%": { boxShadow: "0 0 0 0 rgba(0, 240, 255, 0.7)" },
          "70%": { boxShadow: "0 0 0 10px rgba(0, 240, 255, 0)" },
          "100%": { boxShadow: "0 0 0 0 rgba(0, 240, 255, 0)" },
        },
      },
      animation: {
        shimmer: "shimmer 2s ease-in-out infinite",
        "pulse-red": "pulse-red 2s infinite",
        "pulse-cyan": "pulse-cyan 2s infinite",
      },
    },
  },
  plugins: [],
}
