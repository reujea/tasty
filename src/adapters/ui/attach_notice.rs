//! 저장된 원격 행을 연결하지 못한 이유 안내의 창별 사본. 안내 내용은 App이 넘기고,
//! 이 창의 사이드바 행 표지·레일 칩과 연결 팝업이 읽는다.

use std::collections::HashMap;

use tasty_remote::refusal::RowNoticeKind;

/// 한 워크스페이스의 안내.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AttachNoticeView {
    pub(crate) kind: RowNoticeKind,
}

#[derive(Default)]
pub(crate) struct AttachNotices {
    by_workspace: HashMap<u32, AttachNoticeView>,
    /// 마지막으로 받은 안내 세대. 같으면 다시 받지 않는다.
    generation: Option<u64>,
}

impl AttachNotices {
    pub(crate) fn generation(&self) -> Option<u64> {
        self.generation
    }

    /// 새 세대의 안내로 바꾼다. 내용이 바뀌었으면 true다.
    pub(crate) fn replace(
        &mut self,
        generation: u64,
        notices: HashMap<u32, AttachNoticeView>,
    ) -> bool {
        self.generation = Some(generation);
        if self.by_workspace == notices {
            return false;
        }
        self.by_workspace = notices;
        true
    }

    pub(crate) fn get(&self, workspace_id: u32) -> Option<&AttachNoticeView> {
        self.by_workspace.get(&workspace_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_notices_in_a_new_generation_are_not_a_change() {
        let notice = || {
            HashMap::from([(
                7,
                AttachNoticeView {
                    kind: RowNoticeKind::Occupied,
                },
            )])
        };
        let mut n = AttachNotices::default();
        assert!(n.replace(1, notice()));
        assert!(!n.replace(2, notice()));
        assert_eq!(n.generation(), Some(2));
        assert!(n.get(7).is_some());
        assert!(n.replace(3, HashMap::new()));
        assert!(n.get(7).is_none());
    }
}
