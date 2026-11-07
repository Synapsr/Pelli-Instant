import { COMMANDS, type CullApi, type CullEvents } from '@/lib/contract'
import { toCullError } from './errors'

export type Invoke = (command: string, args: Record<string, unknown>) => Promise<unknown>
export type Subscribe = <E extends keyof CullEvents>(
  event: E,
  handler: (payload: CullEvents[E]) => void,
) => () => void

/**
 * Builds a `CullApi` on top of a generic command transport. Argument objects
 * use the camelCase parameter names of the contract (`{ projectId, photoIds, patch }`).
 */
export function createInvokeApi(rawInvoke: Invoke, on: Subscribe): CullApi {
  async function call<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
    try {
      return (await rawInvoke(command, args)) as T
    } catch (error) {
      throw toCullError(error)
    }
  }

  return {
    appInfo: () => call(COMMANDS.appInfo),
    getSettings: () => call(COMMANDS.getSettings),
    setSettings: (patch) => call(COMMANDS.setSettings, { patch }),
    clearCache: () => call(COMMANDS.clearCache),

    authStatus: () => call(COMMANDS.authStatus),
    authStart: () => call(COMMANDS.authStart),
    authPoll: () => call(COMMANDS.authPoll),
    authLogout: () => call(COMMANDS.authLogout),
    refreshAccount: () => call(COMMANDS.refreshAccount),
    getOffers: (lang) => call(COMMANDS.getOffers, { lang }),

    modelPack: () => call(COMMANDS.modelPack),
    downloadModelPack: () => call(COMMANDS.downloadModelPack),
    cancelModelPack: () => call(COMMANDS.cancelModelPack),
    removeModelPack: () => call(COMMANDS.removeModelPack),

    pickFolder: () => call(COMMANDS.pickFolder),
    listProjects: () => call(COMMANDS.listProjects),
    openFolder: (path, recursive) => call(COMMANDS.openFolder, { path, recursive }),
    removeProject: (projectId) => call(COMMANDS.removeProject, { projectId }),
    getProject: (projectId) => call(COMMANDS.getProject, { projectId }),

    startAnalysis: (projectId, settings) => call(COMMANDS.startAnalysis, { projectId, settings }),
    cancelAnalysis: (projectId) => call(COMMANDS.cancelAnalysis, { projectId }),
    recompute: (projectId, settings) => call(COMMANDS.recompute, { projectId, settings }),

    setMarks: (projectId, photoIds, patch) => call(COMMANDS.setMarks, { projectId, photoIds, patch }),
    largePreview: (projectId, photoId) => call(COMMANDS.largePreview, { projectId, photoId }),
    search: (projectId, query) => call(COMMANDS.search, { projectId, query }),

    exportProject: (projectId, options) => call(COMMANDS.exportProject, { projectId, options }),
    listGalleries: () => call(COMMANDS.listGalleries),
    uploadToPelli: (projectId, request) => call(COMMANDS.uploadToPelli, { projectId, request }),
    cancelUpload: (projectId) => call(COMMANDS.cancelUpload, { projectId }),

    revealInFolder: (path) => call(COMMANDS.revealInFolder, { path }),
    openExternal: (url) => call(COMMANDS.openExternal, { url }),

    updateStatus: () => call(COMMANDS.updateStatus),
    checkForUpdate: () => call(COMMANDS.checkForUpdate),
    installUpdate: () => call(COMMANDS.installUpdate),

    on,
  }
}
