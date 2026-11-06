//! COM and the UI Automation client: an apartment scoped to one
//! observation, the client with its timeouts, and the one cache request
//! every read goes through.

use ::windows::core::{Error, Interface};
use ::windows::Win32::Foundation::RPC_E_CHANGED_MODE;
use ::windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use ::windows::Win32::UI::Accessibility::{
    CUIAutomation8, IUIAutomation, IUIAutomation2, IUIAutomationCacheRequest,
    IUIAutomationCondition, IUIAutomationElement, IUIAutomationElementArray, TreeScope_Children,
    UIA_AutomationIdPropertyId, UIA_ControlTypePropertyId, UIA_IsEnabledPropertyId,
    UIA_NamePropertyId, UIA_ProcessIdPropertyId, UIA_RangeValuePatternId,
    UIA_RangeValueValuePropertyId, UIA_TogglePatternId, UIA_ToggleToggleStatePropertyId,
    UIA_ValuePatternId, UIA_ValueValuePropertyId,
};
use anyhow::{anyhow, Result};

use super::mapping::describe_failure;
use super::MESSAGING_TIMEOUT_MS;

/// Turn a failed call into the sentence `observe` prints (exit 2).
pub(super) fn failed(doing: &'static str) -> impl FnOnce(Error) -> anyhow::Error {
    move |error| anyhow!(describe_failure(doing, error.code().0, &error.message()))
}

/// COM for the calling thread, released on drop. Every interface pointer
/// must be released before it, so `snapshot` enters the apartment first
/// and does all of its UI Automation work in a callee.
pub(super) struct Apartment {
    owned: bool,
}

impl Apartment {
    pub(super) fn enter() -> Result<Self> {
        // SAFETY: the reserved pointer must be null; a success (including
        // S_FALSE, "already initialised") is balanced by `Drop`.
        let status = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if status == RPC_E_CHANGED_MODE {
            // The thread already belongs to a single-threaded apartment
            // that someone else owns: UI Automation works there, and it is
            // not this guard's to release.
            return Ok(Self { owned: false });
        }
        status.ok().map_err(failed("initialising COM"))?;
        Ok(Self { owned: true })
    }
}

impl Drop for Apartment {
    fn drop(&mut self) {
        if self.owned {
            // SAFETY: balances the successful CoInitializeEx in `enter`.
            unsafe { CoUninitialize() };
        }
    }
}

/// Properties fetched for every element in the same round trip as the
/// element itself.
const PROPERTIES: [::windows::Win32::UI::Accessibility::UIA_PROPERTY_ID; 8] = [
    UIA_NamePropertyId,
    UIA_AutomationIdPropertyId,
    UIA_ControlTypePropertyId,
    UIA_IsEnabledPropertyId,
    UIA_ProcessIdPropertyId,
    UIA_ToggleToggleStatePropertyId,
    UIA_ValueValuePropertyId,
    UIA_RangeValueValuePropertyId,
];

/// The UI Automation client, configured for read-only observation.
pub(super) struct Client {
    automation: IUIAutomation,
    cache: IUIAutomationCacheRequest,
    everything: IUIAutomationCondition,
}

impl Client {
    pub(super) fn new() -> Result<Self> {
        // SAFETY: a COM apartment is entered (see `Apartment`), and the
        // CLSID names the in-process UI Automation client.
        let automation: IUIAutomation =
            unsafe { CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER) }
                .map_err(failed("creating the UI Automation client"))?;
        let settings: IUIAutomation2 = automation
            .cast()
            .map_err(failed("configuring the UI Automation client"))?;
        // SAFETY: plain setters on a live client. They only configure
        // this client: how long it waits, and that it never moves focus.
        unsafe {
            settings
                .SetConnectionTimeout(MESSAGING_TIMEOUT_MS)
                .and_then(|()| settings.SetTransactionTimeout(MESSAGING_TIMEOUT_MS))
                .and_then(|()| settings.SetAutoSetFocus(false))
        }
        .map_err(failed("configuring the UI Automation client"))?;
        // SAFETY: factory calls on a live client.
        let everything = unsafe { automation.CreateTrueCondition() }
            .map_err(failed("creating a UI Automation condition"))?;
        // SAFETY: as above.
        let cache = unsafe { automation.CreateCacheRequest() }
            .map_err(failed("creating a UI Automation cache request"))?;
        configure(&cache, &everything).map_err(failed("configuring the UI Automation cache"))?;
        Ok(Self {
            automation,
            cache,
            everything,
        })
    }

    /// The children of `element` in the raw view, every cached property
    /// already fetched: one cross-process round trip per parent instead
    /// of one per property.
    pub(super) fn children(
        &self,
        element: &IUIAutomationElement,
    ) -> ::windows::core::Result<IUIAutomationElementArray> {
        // SAFETY: a find on a live element with this client's condition
        // and cache request; it reads, never acts.
        unsafe { element.FindAllBuildCache(TreeScope_Children, &self.everything, &self.cache) }
    }

    /// The desktop: its children are every top-level window.
    pub(super) fn desktop(&self) -> ::windows::core::Result<IUIAutomationElement> {
        // SAFETY: a read on a live client.
        unsafe { self.automation.GetRootElement() }
    }

    /// The focused element, with the same properties cached.
    pub(super) fn focused(&self) -> ::windows::core::Result<IUIAutomationElement> {
        // SAFETY: a read on a live client.
        unsafe { self.automation.GetFocusedElementBuildCache(&self.cache) }
    }
}

/// Cache every property and pattern the snapshot reads. The true
/// condition as the tree filter makes the cache follow the raw view, the
/// one `FindAll` with a true condition walks.
fn configure(
    cache: &IUIAutomationCacheRequest,
    everything: &IUIAutomationCondition,
) -> ::windows::core::Result<()> {
    // SAFETY: configuring a cache request this client owns.
    unsafe {
        for property in PROPERTIES {
            cache.AddProperty(property)?;
        }
        for pattern in [
            UIA_TogglePatternId,
            UIA_ValuePatternId,
            UIA_RangeValuePatternId,
        ] {
            cache.AddPattern(pattern)?;
        }
        cache.SetTreeFilter(everything)
    }
}
