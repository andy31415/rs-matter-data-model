use matter_data_model::Cluster;
use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;

pub fn server_side_cluster_generate(cluster: &Cluster) -> TokenStream {
    let cluster_name = Ident::new(&cluster.id, Span::call_site());
    quote!(
        struct #cluster_name {

        }
    )
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_tokenstreams_eq::assert_tokenstreams_eq;
    use matter_data_model::Cluster;
    use matter_idl_parser::Idl;
    use quote::quote;

    fn parse_idl(input: &str) -> Idl {
        Idl::parse(input.into()).expect("valid input")
    }

    fn get_cluster_named<'a>(idl: &'a Idl, name: &str) -> Option<&'a Cluster> {
        for cluster in idl.clusters.iter() {
            if cluster.id == name {
                return Some(cluster);
            }
        }
        None
    }

    #[test]
    fn generation_works() {
        let idl = parse_idl(
            "
          cluster UserLabel = 65 {
             revision 1;

             struct LabelStruct {
               char_string<16> label = 0;
               char_string<16> value = 1;
             }

             attribute access(write: manage) LabelStruct labelList[] = 0;
             readonly attribute command_id generatedCommandList[] = 65528;
             readonly attribute command_id acceptedCommandList[] = 65529;
             readonly attribute event_id eventList[] = 65530;
             readonly attribute attrib_id attributeList[] = 65531;
             readonly attribute bitmap32 featureMap = 65532;
             readonly attribute int16u clusterRevision = 65533;
          }
        ",
        );
        let cluster = get_cluster_named(&idl, "UserLabel").expect("Cluster exists");

        assert_tokenstreams_eq!(
            &server_side_cluster_generate(cluster),
            &quote!(
                struct UserLabel {}
            )
        );
    }
}
