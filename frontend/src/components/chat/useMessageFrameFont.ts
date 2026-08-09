import { useMemo } from 'react'
import { useValue } from '@legendapp/state/react'
import { messageFrameFont, type MessageFrameFont } from '../../lib/fonts'
import { settings$ } from '../../states/settings'

/**
 * The message typography a sandboxed body frame should paint with. Frames get
 * pixel sizes because they inherit neither the app's root font size nor the
 * `--me-message-scale` var the in-app message text uses.
 */
export function useMessageFrameFont(bodyBasePx: number, codeBasePx: number): MessageFrameFont {
  const fontFamily = useValue(settings$.fontFamily)
  const messageFontFamily = useValue(settings$.messageFontFamily)
  const fontScale = useValue(settings$.fontScale)
  const messageFontScale = useValue(settings$.messageFontScale)

  return useMemo(
    () => messageFrameFont({ fontFamily, messageFontFamily, fontScale, messageFontScale }, bodyBasePx, codeBasePx),
    [fontFamily, messageFontFamily, fontScale, messageFontScale, bodyBasePx, codeBasePx],
  )
}
