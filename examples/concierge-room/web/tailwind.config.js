/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx,js,jsx}"],
  theme: {
    extend: {
      colors: {
        bg: "#0b1220",
        elev: "#121a2c",
        accent: "#38bdf8",
        accentSoft: "#7dd3fc",
        warn: "#fbbf24",
        err: "#f87171",
        ok: "#4ade80",
      },
    },
  },
  plugins: [],
};
