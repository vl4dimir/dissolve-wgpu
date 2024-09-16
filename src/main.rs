use bytemuck::{Pod, Zeroable};
use std::borrow::Cow;
use std::time::Instant;
use wgpu::util::{BufferInitDescriptor, DeviceExt};
use wgpu::{
    BindGroupDescriptor, BindGroupEntry, BindGroupLayoutDescriptor, BindGroupLayoutEntry,
    BindingResource, BindingType, BufferBinding, BufferBindingType, BufferUsages,
    ComputePassDescriptor, ComputePipelineDescriptor, Extent3d, Limits, MultisampleState,
    ShaderStages, StorageTextureAccess, TextureDescriptor, TextureDimension, TextureFormat,
    TextureUsages, TextureViewDescriptor,
};
use winit::dpi::LogicalSize;
use winit::{
    event::{Event, WindowEvent},
    event_loop::EventLoop,
    window,
    window::Window,
};

#[derive(Copy, Clone, Pod, Zeroable)]
#[repr(C)]
struct Uniforms {
    pub time: f32,
    pub delta_time: f32,
}

async fn run(event_loop: EventLoop<()>, window: Window) {
    let mut size = window.inner_size();
    size.width = size.width.max(1);
    size.height = size.height.max(1);

    let instance = wgpu::Instance::default();

    let surface = instance.create_surface(&window).unwrap();
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
        })
        .await
        .expect("Failed to find an appropriate adapter");

    let (device, queue) = adapter
        .request_device(
            &wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
                required_limits: Limits::default(),
            },
            None,
        )
        .await
        .expect("Failed to create device");

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("render.wgsl"))),
    });

    let compute_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("dissolve.wgsl"))),
    });

    let uniform_bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        entries: &[BindGroupLayoutEntry {
            binding: 0,
            visibility: ShaderStages::VERTEX | ShaderStages::COMPUTE,
            ty: BindingType::Buffer {
                ty: BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
        label: None,
    });

    let texture_bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        entries: &[BindGroupLayoutEntry {
            binding: 0,
            visibility: ShaderStages::COMPUTE,
            ty: BindingType::StorageTexture {
                access: StorageTextureAccess::WriteOnly,
                format: TextureFormat::Rgba16Float,
                view_dimension: Default::default(),
            },
            count: None,
        }],
        label: None,
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[&uniform_bind_group_layout],
        push_constant_ranges: &[],
    });

    let surface_format = TextureFormat::Rgba16Float;

    let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: "vs_main",
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: "fs_main",
            compilation_options: Default::default(),
            targets: &[Some(surface_format.into())],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: MultisampleState {
            count: 4,
            mask: 0,
            alpha_to_coverage_enabled: false,
        },
        multiview: None,
    });

    let compute_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[&uniform_bind_group_layout, &texture_bind_group_layout],
        push_constant_ranges: &[],
    });

    let compute_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
        label: None,
        layout: Some(&compute_pipeline_layout),
        module: &compute_shader,
        entry_point: "dissolve",
        compilation_options: Default::default(),
    });

    let uniforms = Uniforms {
        time: 0.0,
        delta_time: 0.0,
    };
    let uniform_buffer = device.create_buffer_init(&BufferInitDescriptor {
        label: None,
        contents: bytemuck::bytes_of(&uniforms),
        usage: BufferUsages::COPY_DST | BufferUsages::UNIFORM,
    });

    let uniform_bind_group = device.create_bind_group(&BindGroupDescriptor {
        label: None,
        layout: &uniform_bind_group_layout,
        entries: &[BindGroupEntry {
            binding: 0,
            resource: BindingResource::Buffer(BufferBinding {
                buffer: &uniform_buffer,
                offset: 0,
                size: None,
            }),
        }],
    });

    let mut config = surface
        .get_default_config(&adapter, size.width, size.height)
        .unwrap();
    config.usage = TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_DST;
    config.format = surface_format;
    surface.configure(&device, &config);

    let size = Extent3d {
        width: size.width,
        height: size.height,
        depth_or_array_layers: 1,
    };
    let mut multisampled_texture = device.create_texture(&TextureDescriptor {
        size,
        mip_level_count: 1,
        sample_count: 4,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba16Float,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_DST,
        label: None,
        view_formats: &[],
    });
    let mut multisampled_texture_view =
        multisampled_texture.create_view(&TextureViewDescriptor::default());

    let mut texture = device.create_texture(&TextureDescriptor {
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba16Float,
        usage: TextureUsages::RENDER_ATTACHMENT
            | TextureUsages::COPY_SRC
            | TextureUsages::STORAGE_BINDING,
        label: None,
        view_formats: &[],
    });
    let mut texture_view = texture.create_view(&TextureViewDescriptor::default());

    let mut texture_bind_group = device.create_bind_group(&BindGroupDescriptor {
        label: None,
        layout: &texture_bind_group_layout,
        entries: &[BindGroupEntry {
            binding: 0,
            resource: BindingResource::TextureView(&texture_view),
        }],
    });

    let start_instant = Instant::now();
    let mut last_render_instant = Instant::now();
    let window = &window;
    event_loop
        .run(move |event, target| {
            let _ = (
                &instance,
                &adapter,
                &shader,
                &pipeline_layout,
                &uniform_buffer,
                &multisampled_texture,
                &multisampled_texture_view,
                &texture,
                &texture_view,
            );

            if let Event::WindowEvent {
                window_id: _,
                event,
            } = event
            {
                match event {
                    WindowEvent::Resized(new_size) => {
                        println!("Resizing to {:?}", new_size);
                        config.width = new_size.width.max(1);
                        config.height = new_size.height.max(1);
                        surface.configure(&device, &config);

                        multisampled_texture = device.create_texture(&TextureDescriptor {
                            size,
                            mip_level_count: 1,
                            sample_count: 4,
                            dimension: TextureDimension::D2,
                            format: TextureFormat::Rgba16Float,
                            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_DST,
                            label: None,
                            view_formats: &[],
                        });
                        multisampled_texture_view =
                            multisampled_texture.create_view(&TextureViewDescriptor::default());

                        texture = device.create_texture(&TextureDescriptor {
                            size,
                            mip_level_count: 1,
                            sample_count: 1,
                            dimension: TextureDimension::D2,
                            format: TextureFormat::Rgba16Float,
                            usage: TextureUsages::RENDER_ATTACHMENT
                                | TextureUsages::COPY_SRC
                                | TextureUsages::STORAGE_BINDING,
                            label: None,
                            view_formats: &[],
                        });
                        texture_view = texture.create_view(&TextureViewDescriptor::default());

                        texture_bind_group = device.create_bind_group(&BindGroupDescriptor {
                            label: None,
                            layout: &texture_bind_group_layout,
                            entries: &[BindGroupEntry {
                                binding: 0,
                                resource: BindingResource::TextureView(&texture_view),
                            }],
                        });

                        window.request_redraw();
                    }
                    WindowEvent::RedrawRequested => {
                        let now = Instant::now();
                        let time = (now - start_instant).as_secs_f32();
                        let delta_time = (now - last_render_instant).as_secs_f32();
                        let uniforms = Uniforms { time, delta_time };
                        queue.write_buffer(&uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

                        let current_texture = surface
                            .get_current_texture()
                            .expect("Failed to acquire current surface texture");
                        let mut command_encoder =
                            device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                                label: None,
                            });

                        // Run compute – "dissolve" the texture by setting random pixels to black
                        {
                            let mut compute_pass =
                                command_encoder.begin_compute_pass(&ComputePassDescriptor {
                                    label: None,
                                    timestamp_writes: None,
                                });
                            compute_pass.set_pipeline(&compute_pipeline);
                            compute_pass.set_bind_group(0, &uniform_bind_group, &[]);
                            compute_pass.set_bind_group(1, &texture_bind_group, &[]);
                            compute_pass.dispatch_workgroups(
                                (config.width as f32 / 8f32).ceil() as u32,
                                (config.height as f32 / 8f32).ceil() as u32,
                                1,
                            );
                        }

                        // Since the target texture has been modified by the compute shader, we must
                        // copy the modifications "back" into the multisampled texture, so that the
                        // upcoming render pass will observe them.
                        command_encoder.copy_texture_to_texture(
                            texture.as_image_copy(),
                            multisampled_texture.as_image_copy(),
                            texture.size(),
                        );

                        // Render the rotating triangle
                        {
                            let mut render_pass =
                                command_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                    label: None,
                                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                        view: &multisampled_texture_view,
                                        resolve_target: Some(&texture_view),
                                        ops: wgpu::Operations {
                                            load: wgpu::LoadOp::Load, // Crucial for this technique
                                            store: wgpu::StoreOp::Store,
                                        },
                                    })],
                                    depth_stencil_attachment: None,
                                    timestamp_writes: None,
                                    occlusion_query_set: None,
                                });
                            render_pass.set_bind_group(0, &uniform_bind_group, &[]);
                            render_pass.set_pipeline(&render_pipeline);
                            render_pass.draw(0..3, 0..1);
                        }

                        command_encoder.copy_texture_to_texture(
                            texture.as_image_copy(),
                            current_texture.texture.as_image_copy(),
                            texture.size(),
                        );

                        queue.submit(Some(command_encoder.finish()));
                        current_texture.present();

                        last_render_instant = now;
                        window.request_redraw();
                    }
                    WindowEvent::CloseRequested => target.exit(),
                    _ => {}
                };
            }
        })
        .unwrap();
}

pub fn main() {
    let event_loop = EventLoop::new().unwrap();
    let window = window::WindowBuilder::new()
        .with_inner_size(LogicalSize {
            width: 640,
            height: 640,
        })
        .build(&event_loop)
        .unwrap();

    pollster::block_on(run(event_loop, window));
}
