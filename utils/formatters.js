const toTitleCase = (str) =>
  str
    .split('_')
    .join(' ')
    .replace(
      /\w\S*/g,
      (txt) => txt.charAt(0).toUpperCase() + txt.substr(1).toLowerCase()
    )

export { toTitleCase }
