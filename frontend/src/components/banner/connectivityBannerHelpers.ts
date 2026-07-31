type AccountLabelSource = {
  id: string
  email: string
  display_name: string
}

export function connectivityAccountLabel(accountId: string | null, accounts: AccountLabelSource[]): string | null {
  if (!accountId) return null

  const account = accounts.find((candidate) => candidate.id === accountId)
  if (!account) return accountId

  const displayName = account.display_name.trim()
  const email = account.email.trim()
  if (!displayName) return email || account.id

  const duplicateName = accounts.some(
    (candidate) =>
      candidate.id !== account.id &&
      candidate.display_name.trim().localeCompare(displayName, undefined, { sensitivity: 'accent' }) === 0,
  )
  return duplicateName && email && email.localeCompare(displayName, undefined, { sensitivity: 'accent' }) !== 0
    ? `${displayName} (${email})`
    : displayName
}

// Core errors add one of these contexts around failures opening or negotiating
// the proxy connection. Extract only the host:port; the rest remains diagnostic
// detail in the log and may be too technical for the banner.
export function proxyEndpointFromSyncError(error: string): string | null {
  const endpoint = String.raw`(\[[^\]]+\]:\d+|[^\s:]+:\d+)`
  return (
    new RegExp(String.raw`\bconnect to proxy\s+${endpoint}`, 'i').exec(error)?.[1] ??
    new RegExp(String.raw`\b(?:http|socks5) proxy\s+${endpoint}`, 'i').exec(error)?.[1] ??
    null
  )
}
