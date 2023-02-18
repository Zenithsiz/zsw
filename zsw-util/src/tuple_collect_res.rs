//! Tuple collect results

use Result as R;

#[duplicate::duplicate_item(
		Name                 ty                     unwrap                      res;
		[ TupleCollectRes1 ] [ T1                 ] [ T1?                     ] [ R<T1, E>                                         ];
		[ TupleCollectRes2 ] [ T1, T2             ] [ T1?, T2?                ] [ R<T1, E>, R<T2, E>                               ];
		[ TupleCollectRes3 ] [ T1, T2, T3         ] [ T1?, T2?, T3?           ] [ R<T1, E>, R<T2, E>, R<T3, E>                     ];
		[ TupleCollectRes4 ] [ T1, T2, T3, T4     ] [ T1?, T2?, T3?, T4?      ] [ R<T1, E>, R<T2, E>, R<T3, E>, R<T4, E>           ];
		[ TupleCollectRes5 ] [ T1, T2, T3, T4, T5 ] [ T1?, T2?, T3?, T4?, T5? ] [ R<T1, E>, R<T2, E>, R<T3, E>, R<T4, E>, R<T5, E> ];
	)]
#[extend::ext(name = Name)]
pub impl<ty, E> (res,) {
	fn collect_result(self) -> Result<(ty,), E> {
		#[allow(non_snake_case)] // Simplifies macro
		let (ty,) = self;
		Ok((unwrap,))
	}
}
