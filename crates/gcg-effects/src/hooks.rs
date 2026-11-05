//! Typed parameters of the 34 custom hooks (docs/port/PLAN.md §5.3; Inv 04 §6.2), decoded
//! once at load from the node's raw `params`.
//!
//! Python reads `dict(params)` on every call (`interp.py:1573`, `view.py:781,882,933`), so a
//! repeated key is last-wins and an unused key is ignored; both rules hold here. A parameter
//! the hook body would reject (a missing key, `KeyError`; a wrong type, the bodies'
//! `assert isinstance` or `TypeError`) fails the load instead of the first call, so no card
//! with an unusable hook loads silently (W-69). The raw `params` stay in the node for `repr`.

use crate::codec::Segments;
use crate::contracts::{
    CondId, CondParams, FilterParams, HookParams, IrError, NodeId, ParamValue, SelId, StepParams,
    StrId, ValueParams, VarId,
};
use crate::dsl::{CondHook, FilterHook, Node, NodeKind, P, StepHook, ValueHook};

/// What the decoder needs to type a hook's parameters.
pub(crate) struct ParamCx<'a> {
    pub strings: &'a [Box<str>],
    pub var_names: Segments<'a>,
    pub kinds: &'a [NodeKind],
    /// JSON path of the node, for errors.
    pub at: String,
}

impl ParamCx<'_> {
    fn err(&self, detail: impl Into<String>) -> IrError {
        IrError::Malformed {
            path: format!("{}.params", self.at),
            detail: detail.into(),
        }
    }

    /// `dict(params).get(key)`: the last pair with that key.
    fn get(&self, params: &[(StrId, ParamValue)], key: &str) -> Option<ParamValue> {
        params
            .iter()
            .rev()
            .find(|(k, _)| &*self.strings[k.0 as usize] == key)
            .map(|(_, v)| *v)
    }

    fn required(&self, params: &[(StrId, ParamValue)], key: &str) -> Result<ParamValue, IrError> {
        self.get(params, key)
            .ok_or_else(|| self.err(format!("missing parameter {key:?}")))
    }

    /// `str(params[key])` of a string parameter.
    fn text(&self, params: &[(StrId, ParamValue)], key: &str) -> Result<StrId, IrError> {
        match self.required(params, key)? {
            ParamValue::Str(s) => Ok(s),
            other => Err(self.err(format!("parameter {key:?} must be a str, got {other:?}"))),
        }
    }

    /// A string parameter used as a frame-variable name (`f.vars[...]`).
    fn var(&self, params: &[(StrId, ParamValue)], key: &str) -> Result<VarId, IrError> {
        let s = self.text(params, key)?;
        let name = &self.strings[s.0 as usize];
        self.var_names
            .find(name)
            .and_then(|i| u16::try_from(i).ok())
            .map(VarId)
            .ok_or_else(|| self.err(format!("variable {name:?} is not in var_names")))
    }

    /// An int parameter (`_int_param`, `wp_gd05_a.py:86-90`; `assert isinstance(.., int)`).
    fn int(&self, params: &[(StrId, ParamValue)], key: &str) -> Result<i64, IrError> {
        match self.required(params, key)? {
            ParamValue::Int(n) => Ok(n),
            other => Err(self.err(format!("parameter {key:?} must be an int, got {other:?}"))),
        }
    }

    fn node(
        &self,
        params: &[(StrId, ParamValue)],
        key: &str,
        kinds: &[NodeKind],
    ) -> Result<NodeId, IrError> {
        match self.required(params, key)? {
            ParamValue::Node(id) if kinds.contains(&self.kinds[id.0 as usize]) => Ok(id),
            other => Err(self.err(format!("parameter {key:?} has the wrong kind: {other:?}"))),
        }
    }

    fn sel(&self, params: &[(StrId, ParamValue)], key: &str) -> Result<SelId, IrError> {
        self.node(params, key, &[NodeKind::Sel]).map(SelId)
    }

    fn player(&self, params: &[(StrId, ParamValue)], key: &str) -> Result<P, IrError> {
        let s = self.text(params, key)?;
        let value = &self.strings[s.0 as usize];
        P::from_value(value).ok_or_else(|| self.err(format!("{value:?} is not a P value")))
    }
}

/// The typed parameters of a `Custom*` node, or `None` for any other node.
pub(crate) fn hook_params(node: &Node, cx: &ParamCx<'_>) -> Result<Option<HookParams>, IrError> {
    Ok(Some(match node {
        Node::CustomStep(x) => HookParams::Step(step(x.name, &x.params, cx)?),
        Node::CustomCond(x) => HookParams::Cond(cond(x.name, &x.params, cx)?),
        Node::CustomFilter(x) => HookParams::Filter(filter(x.name, &x.params, cx)?),
        Node::CustomValue(x) => HookParams::Value(value(x.name, &x.params, cx)?),
        _ => return Ok(None),
    }))
}

/// The 13 step hooks (`interp.CUSTOM_STEPS`).
fn step(
    hook: StepHook,
    params: &[(StrId, ParamValue)],
    cx: &ParamCx<'_>,
) -> Result<StepParams, IrError> {
    Ok(match hook {
        StepHook::ReturnLookedBottom => StepParams::ReturnLookedBottom,
        StepHook::DeployExBase => StepParams::DeployExBase,
        StepHook::SupportUsed => StepParams::SupportUsed,
        // wp_eb01_a.py:64,68: d.P(str(params["player"])), str(params["var"])
        StepHook::EB01aLookOwnTop => StepParams::EB01aLookOwnTop {
            player: cx.player(params, "player")?,
            var: cx.var(params, "var")?,
        },
        // wp_eb01_b.py:243,247
        StepHook::EB01bLookTopOwn => StepParams::EB01bLookTopOwn {
            player: cx.player(params, "player")?,
            var: cx.var(params, "var")?,
        },
        // wp_gd02_b.py:59
        StepHook::GD02bPayDeployCost => StepParams::GD02bPayDeployCost {
            var: cx.var(params, "var")?,
        },
        // wp_gd03_a.py:99-100: var = params["var"]; assert isinstance(var, str)
        StepHook::GD03aPayCostOf => StepParams::GD03aPayCostOf {
            var: cx.var(params, "var")?,
        },
        // wp_gd03_a.py:117-120
        StepHook::GD03aApMinusCount => StepParams::GD03aApMinusCount {
            var: cx.var(params, "var")?,
            sel: cx.sel(params, "sel")?,
        },
        // wp_gd04_a.py:72
        StepHook::GD04aCopyKeywords => StepParams::GD04aCopyKeywords {
            var: cx.var(params, "var")?,
        },
        // wp_gd04_a.py:90,94
        StepHook::GD04aApDownByNamedTrashUnits => StepParams::GD04aApDownByNamedTrashUnits {
            name: cx.text(params, "name")?,
            var: cx.var(params, "var")?,
        },
        // wp_gd05_a.py:118
        StepHook::GD05aUseOnce => StepParams::GD05aUseOnce {
            tag: cx.int(params, "tag")?,
        },
        // wp_gd05_b.py:76-77
        StepHook::GD05bDestroyFirstShieldAreaCards => {
            StepParams::GD05bDestroyFirstShieldAreaCards {
                count: cx.int(params, "count")?,
            }
        }
        StepHook::GD05bResolvingCommandToTrash => StepParams::GD05bResolvingCommandToTrash,
    })
}

/// The 7 condition hooks (`view.CUSTOM_CONDS`).
fn cond(
    hook: CondHook,
    params: &[(StrId, ParamValue)],
    cx: &ParamCx<'_>,
) -> Result<CondParams, IrError> {
    Ok(match hook {
        CondHook::AttackTargetDamaged => CondParams::AttackTargetDamaged,
        CondHook::GD01bDestroyedWithDamage => CondParams::GD01bDestroyedWithDamage,
        CondHook::GD04bPlayedWithEx => CondParams::GD04bPlayedWithEx,
        // wp_gd05_a.py:113
        CondHook::GD05aOnceFree => CondParams::GD05aOnceFree {
            tag: cx.int(params, "tag")?,
        },
        // wp_gd05_a.py:131: params.get("by") != "you" (any non-str value compares unequal)
        CondHook::GD05aDestroyedByEffect => CondParams::GD05aDestroyedByEffect {
            by_you: matches!(
                cx.get(params, "by"),
                Some(ParamValue::Str(s)) if &*cx.strings[s.0 as usize] == "you"
            ),
        },
        CondHook::GD05bPlayedWithEx => CondParams::GD05bPlayedWithEx,
        CondHook::ST12DestroyedByEffectDamage => CondParams::ST12DestroyedByEffectDamage,
    })
}

/// The 9 filter hooks (`view.CUSTOM_FILTERS`).
fn filter(
    hook: FilterHook,
    params: &[(StrId, ParamValue)],
    cx: &ParamCx<'_>,
) -> Result<FilterParams, IrError> {
    Ok(match hook {
        FilterHook::IsAttackTarget => FilterParams::IsAttackTarget,
        FilterHook::GD03aHasDestroyedEffect => FilterParams::GD03aHasDestroyedEffect,
        FilterHook::GD03aLowestHp => FilterParams::GD03aLowestHp,
        // wp_gd03_b.py:69: cast(d.Cond, params["cond"])
        FilterHook::GD03bCondHolds => FilterParams::GD03bCondHolds {
            cond: CondId(cx.node(params, "cond", NodeKind::COND)?),
        },
        FilterHook::GD03bBattlingEnemyUnit => FilterParams::GD03bBattlingEnemyUnit,
        FilterHook::GD04aPilotCapable => FilterParams::GD04aPilotCapable,
        FilterHook::GD04aHasPrintedKeyword => FilterParams::GD04aHasPrintedKeyword,
        // wp_gd04_a.py:118: str(params["trait"])
        FilterHook::GD04aHasPartnerInTrash => FilterParams::GD04aHasPartnerInTrash {
            r#trait: cx.text(params, "trait")?,
        },
        FilterHook::GD05bCanPair => FilterParams::GD05bCanPair,
    })
}

/// The 5 value hooks (`view.CUSTOM_VALUES`).
fn value(
    hook: ValueHook,
    params: &[(StrId, ParamValue)],
    cx: &ParamCx<'_>,
) -> Result<ValueParams, IrError> {
    Ok(match hook {
        // wp_gd03_a.py:55-56: per = params["per"]; assert isinstance(per, int)
        ValueHook::GD03aApPer => ValueParams::GD03aApPer {
            per: cx.int(params, "per")?,
        },
        // wp_gd03_b.py:99-100
        ValueHook::GD03bUniqueNames => ValueParams::GD03bUniqueNames {
            sel: cx.sel(params, "sel")?,
        },
        ValueHook::GD05aLowestEnemyUnitLevel => ValueParams::GD05aLowestEnemyUnitLevel,
        ValueHook::ST05HighestEnemyUnitLv => ValueParams::ST05HighestEnemyUnitLv,
        ValueHook::ST12LowestRestedEnemyLv => ValueParams::ST12LowestRestedEnemyLv,
    })
}
