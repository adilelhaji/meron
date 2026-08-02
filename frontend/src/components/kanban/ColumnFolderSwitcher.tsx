import { useEffect, useMemo, useState, type MouseEvent as ReactMouseEvent } from 'react'
import { useValue } from '@legendapp/state/react'
import { Check, ChevronDown, ChevronRight } from 'lucide-react'
import { folderIcon } from '../../lib/folderIcon'
import { useTranslation } from '../../lib/i18n'
import { clsx } from '../../lib/utils'
import { ensureAccountFolders, mail$ } from '../../states/mail'
import type { Folder } from '../../types'
import { FloatingContextMenu } from '../menu/FloatingContextMenu'
import { menuItemBase } from '../menu/menuStyles'
import { buildFolderTree, type TreeNode } from './folderTree'

const FILTER_THRESHOLD = 8

// One folder in the picker tree: the expander, the folder row itself and, when
// expanded, its children. Structural nodes (a path segment with no folder of its
// own) are shown but not selectable.
function FolderNodeRow({
  node,
  depth,
  currentFolderId,
  takenFolderIds,
  onPick,
}: {
  node: TreeNode
  depth: number
  currentFolderId: string
  takenFolderIds?: string[]
  onPick: (folderId: string) => void
}) {
  const [expanded, setExpanded] = useState(true)
  const hasChildren = node.children.length > 0
  const current = !!node.folder && node.folder.id === currentFolderId
  const taken = !!node.folder && !current && !!takenFolderIds?.includes(node.folder.id)
  const selectable = !!node.folder && !current && !taken
  const Icon = folderIcon(node.folder)

  return (
    <div>
      <div className="flex items-center" style={{ paddingLeft: depth * 14 }}>
        <button
          type="button"
          className={clsx(
            'flex h-8 w-5 shrink-0 items-center justify-center rounded text-secondary',
            hasChildren ? 'cursor-pointer hover:text-primary' : 'invisible',
          )}
          tabIndex={hasChildren ? 0 : -1}
          onClick={() => setExpanded((open) => !open)}
        >
          <ChevronRight size={13} className={clsx('transition-transform', expanded && 'rotate-90')} />
        </button>
        <button
          type="button"
          disabled={!selectable}
          className={clsx(
            menuItemBase,
            'min-w-0 flex-1',
            current ? 'font-semibold text-accent' : 'text-primary',
            selectable ? 'hover:bg-hover' : 'cursor-default',
            taken && 'opacity-40',
            !node.folder && 'text-secondary',
          )}
          onClick={() => node.folder && onPick(node.folder.id)}
        >
          {current ? (
            <Check size={13} className="shrink-0 text-accent" />
          ) : (
            <Icon size={13} className="shrink-0 text-secondary" />
          )}
          <span className="min-w-0 truncate">{node.name}</span>
        </button>
      </div>
      {hasChildren && expanded && (
        <div>
          {node.children.map((child) => (
            <FolderNodeRow
              key={child.folder?.id ?? `${depth}-${child.name}`}
              node={child}
              depth={depth + 1}
              currentFolderId={currentFolderId}
              takenFolderIds={takenFolderIds}
              onPick={onPick}
            />
          ))}
        </div>
      )}
    </div>
  )
}

// The column header's folder name, doubling as a picker: clicking it lists the
// other folders of the same account so the column can be pointed elsewhere
// without removing and re-adding it.
export function ColumnFolderSwitcher({
  accountId,
  folderId,
  label,
  labelClassName,
  takenFolderIds,
  onSelect,
}: {
  accountId: string
  folderId: string
  label: string
  labelClassName?: string
  /** Folders that already have their own column and so can't be switched to. */
  takenFolderIds?: string[]
  onSelect: (folderId: string) => void
}) {
  const { t } = useTranslation()
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null)
  // Keep observing the shared cache: when it contains only the bootstrap Inbox,
  // ensureAccountFolders refreshes it in the background.
  const folders = useValue(mail$.foldersByAccount[accountId]) ?? []
  const [loading, setLoading] = useState(false)
  const [query, setQuery] = useState('')

  useEffect(() => {
    if (!menu) return
    let cancelled = false
    setLoading(true)
    void ensureAccountFolders(accountId, { refreshIfBootstrapOnly: true }).finally(() => {
      if (!cancelled) setLoading(false)
    })
    return () => {
      cancelled = true
    }
  }, [menu, accountId])

  const close = () => {
    setMenu(null)
    setQuery('')
  }

  const open = (event: ReactMouseEvent<HTMLButtonElement>) => {
    event.preventDefault()
    event.stopPropagation()
    if (menu) {
      close()
      return
    }
    const rect = event.currentTarget.getBoundingClientRect()
    setMenu({ x: rect.left, y: rect.bottom })
  }

  const needle = query.trim().toLowerCase()
  // Filtering narrows the folder set, then the tree is rebuilt from what's left,
  // so matches keep the hierarchy they sit in.
  const tree = useMemo(
    () => buildFolderTree(folders.filter((folder) => !needle || folder.name.toLowerCase().includes(needle))),
    [folders, needle],
  )
  const showFilter = folders.length > FILTER_THRESHOLD

  return (
    <>
      <button
        type="button"
        // Sized well past the label's own line box: the header is 48px tall, so a
        // text-height hit target left most of it dead.
        className={clsx('flex h-8 min-w-0 items-center gap-1 rounded px-2 -mx-2 hover:bg-hover', labelClassName)}
        title={t('kanban.actions.switchFolder')}
        onClick={open}
        // The header is a drag handle; keep the pointer gesture to ourselves.
        onPointerDown={(event) => event.stopPropagation()}
        onContextMenu={(event) => event.stopPropagation()}
      >
        <span className="truncate">{label}</span>
        <ChevronDown size={12} className="shrink-0 text-secondary" />
      </button>
      {menu && (
        <FloatingContextMenu
          x={menu.x}
          y={menu.y}
          offset={2}
          onClose={close}
          overlay
          className="fixed z-50 flex max-h-[min(420px,calc(100vh-1rem))] w-60 flex-col rounded-xl border border-border bg-chats p-1 shadow-2xl animate-fade-in text-primary"
          onContextMenu={(event) => {
            event.preventDefault()
            event.stopPropagation()
          }}
        >
          {showFilter && (
            <input
              autoFocus
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Escape') close()
              }}
              placeholder={t('folders.searchPlaceholder')}
              className="mb-1 h-8 w-full shrink-0 rounded-lg bg-hover px-2 text-[13px] text-primary outline-none placeholder-secondary focus:ring-1 focus:ring-accent/40"
            />
          )}
          <div className="min-h-0 flex-1 overflow-y-auto">
            {tree.length === 0 ? (
              <div className="px-2 py-4 text-center text-xs font-medium text-secondary">
                {loading ? t('folders.loading') : t('folders.noneAvailable')}
              </div>
            ) : (
              tree.map((node) => (
                <FolderNodeRow
                  key={node.folder?.id ?? node.name}
                  node={node}
                  depth={0}
                  currentFolderId={folderId}
                  takenFolderIds={takenFolderIds}
                  onPick={(picked) => {
                    close()
                    onSelect(picked)
                  }}
                />
              ))
            )}
          </div>
        </FloatingContextMenu>
      )}
    </>
  )
}
