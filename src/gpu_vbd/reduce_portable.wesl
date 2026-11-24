import package::reduce_state::{energies, gradients, hessians};

// Portable 32-lane reductions; every workgroup barrier is required.
public fn reduce_system(e:f32,g:vec3f,h:mat3x3f,lane:u32) {
    energies[lane]=e;gradients[lane]=g;hessians[lane]=h;
    workgroupBarrier();
    for(var stride=16u;stride>0u;stride/=2u) {
        if lane<stride {energies[lane]+=energies[lane+stride];gradients[lane]+=gradients[lane+stride];hessians[lane]+=hessians[lane+stride];}
        workgroupBarrier();
    }
}
public fn reduce_energy(e:f32,lane:u32) {
    energies[lane]=e;workgroupBarrier();
    for(var stride=16u;stride>0u;stride/=2u){
        if lane<stride {energies[lane]+=energies[lane+stride];}
        workgroupBarrier();
    }
}
