# Rebuild the supplied pixel artwork without its baked checkerboard/shadow.
# shortcut: sampling targets this artwork's 100-cell grid; rework it if the source design changes.
$ErrorActionPreference='Stop'
$root=(Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$out=Join-Path $root 'target/icon-assets'
New-Item -ItemType Directory -Force $out | Out-Null
Add-Type -AssemblyName System.Drawing
Add-Type -ReferencedAssemblies System.Drawing -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Drawing.Imaging;
using System.IO;
public static class RustyIcon {
    static Color Rgb(int rgb) { return Color.FromArgb(255, rgb >> 16 & 255, rgb >> 8 & 255, rgb & 255); }
    static GraphicsPath Tile(float inset) {
        var points=new PointF[256]; double radius=48-inset;
        for (int i=0; i<points.Length; i++) {
            double angle=2*Math.PI*i/points.Length, x=Math.Cos(angle), y=Math.Sin(angle);
            points[i]=new PointF((float)(50+radius*Math.Sign(x)*Math.Sqrt(Math.Abs(x))),
                (float)(50+radius*Math.Sign(y)*Math.Sqrt(Math.Abs(y))));
        }
        var p=new GraphicsPath(); p.AddPolygon(points); return p;
    }
    static void Glow(Graphics g, RectangleF bounds, Color center) {
        using (var path=new GraphicsPath()) {
            path.AddEllipse(bounds);
            using (var brush=new PathGradientBrush(path)) {
                brush.CenterColor=center; brush.SurroundColors=new[]{Color.FromArgb(0,center)};
                g.FillPath(brush,path);
            }
        }
    }
    public static void Build(string source, string output) {
        int[] palette = {0x38120c, 0x592316, 0x7b321b, 0x984221, 0xb55225,
            0xcf6829, 0xe68746, 0xf2ad79, 0xf6c298, 0xcf916e, 0xac7159, 0x8b5140};
        var pixels = new Color[100,100];
        int left=100, top=100, right=0, bottom=0, count=0;
        using (var input = new Bitmap(source)) {
            if (input.Width != input.Height || input.Width < 100) throw new Exception("Expected square source artwork");
            for (int y=10; y<87; y++) for (int x=14; x<87; x++) {
                Color c=input.GetPixel((int)((x+.5)*input.Width/100), (int)((y+.5)*input.Height/100));
                // Rust pixels only: the gray checkerboard, charcoal tile and sparkle stay out.
                if (c.R < 48 || c.R-c.G < 24 || c.R-c.B < 32) continue;
                Color chosen=Color.Empty; int best=int.MaxValue;
                foreach (int rgb in palette) {
                    Color p=Rgb(rgb); int dr=c.R-p.R, dg=c.G-p.G, db=c.B-p.B;
                    int distance=dr*dr+dg*dg+db*db;
                    if (distance<best) { best=distance; chosen=p; }
                }
                pixels[x,y]=chosen; count++;
                left=Math.Min(left,x); right=Math.Max(right,x); top=Math.Min(top,y); bottom=Math.Max(bottom,y);
            }
        }
        if (count<2000 || count>6000 || right-left<40 || bottom-top<50) throw new Exception("Rust paper mask changed unexpectedly");
        float cell=80f/Math.Max(right-left+1, bottom-top+1);
        float ox=(100-(right-left+1)*cell)/2, oy=(100-(bottom-top+1)*cell)/2-1;
        using (var master = new Bitmap(1024,1024,PixelFormat.Format32bppArgb)) {
            using (Graphics g=Graphics.FromImage(master)) {
                g.Clear(Color.Transparent); g.ScaleTransform(10.24f,10.24f); g.SmoothingMode=SmoothingMode.AntiAlias;
                using (var tile=Tile(0)) {
                    using (var glass=new LinearGradientBrush(new RectangleF(0,0,100,100),
                        Color.FromArgb(236,78,71,65),Color.FromArgb(230,26,23,21),55f)) g.FillPath(glass,tile);
                    g.SetClip(tile);
                    Glow(g,new RectangleF(-28,-38,146,130),Color.FromArgb(56,255,238,213));
                    Glow(g,new RectangleF(57,57,65,68),Color.FromArgb(45,232,128,67));
                    g.ResetClip();
                    using (var rim=new LinearGradientBrush(new RectangleF(0,0,100,100),
                        Color.FromArgb(190,255,246,231),Color.FromArgb(35,239,178,125),55f))
                    using (var edge=new Pen(rim,.8f)) g.DrawPath(edge,tile);
                    using (var inner=Tile(1.5f))
                    using (var rim=new LinearGradientBrush(new RectangleF(0,0,100,100),
                        Color.FromArgb(48,255,250,240),Color.FromArgb(8,255,230,204),90f))
                    using (var edge=new Pen(rim,.5f)) g.DrawPath(edge,inner);
                }
                g.SmoothingMode=SmoothingMode.None;
                for (int layer=3; layer>=0; layer--) {
                    float spread=layer*.55f;
                    using (var shadow=new SolidBrush(Color.FromArgb(layer==0?65:7,15,10,7))) {
                        for (int y=top; y<=bottom; y++) for (int x=left; x<=right; x++) {
                            if (!pixels[x,y].IsEmpty) g.FillRectangle(shadow,ox+(x-left)*cell+.8f-spread,
                                oy+(y-top)*cell+1.9f-spread,cell+2*spread,cell+2*spread);
                        }
                    }
                }
                for (int y=top; y<=bottom; y++) for (int x=left; x<=right; x++) {
                    if (pixels[x,y].IsEmpty) continue;
                    using (var brush=new SolidBrush(pixels[x,y])) g.FillRectangle(brush,ox+(x-left)*cell,oy+(y-top)*cell,cell,cell);
                }
                g.SmoothingMode=SmoothingMode.AntiAlias;
                using (var tile=Tile(0)) {
                    g.SetClip(tile);
                    using (var sheen=new LinearGradientBrush(new RectangleF(-15,-48,146,112),
                        Color.FromArgb(28,255,248,235),Color.FromArgb(0,255,248,235),90f))
                        g.FillEllipse(sheen,-15,-48,146,112);
                    g.ResetClip();
                }
            }
            if (master.GetPixel(80,250).R <= master.GetPixel(80,780).R+10) throw new Exception("Glass lighting self-check failed");
            master.Save(Path.Combine(output,"rusty-note.png"),ImageFormat.Png);
            int[] sizes={16,20,24,32,48,64,128,256}; var frames=new List<byte[]>();
            using (var sheet=new Bitmap(720,300,PixelFormat.Format32bppArgb)) using (var preview=Graphics.FromImage(sheet)) {
                preview.Clear(Rgb(0xfbf6ef));
                using (var dark=new SolidBrush(Rgb(0x191716))) preview.FillRectangle(dark,0,150,720,150);
                int px=16;
                foreach (int size in sizes) {
                    using (var frame=new Bitmap(size,size,PixelFormat.Format32bppArgb)) {
                        using (var g=Graphics.FromImage(frame)) {
                            g.InterpolationMode=InterpolationMode.HighQualityBicubic; g.PixelOffsetMode=PixelOffsetMode.HighQuality;
                            g.DrawImage(master,new Rectangle(0,0,size,size));
                        }
                        int glassAlpha=frame.GetPixel(Math.Max(1,size/12),size/2).A;
                        if (frame.GetPixel(0,0).A != 0 || frame.GetPixel(size/2,size/2).A != 255 || glassAlpha==0 || glassAlpha==255)
                            throw new Exception("Transparent corners, translucent glass or opaque paper self-check failed");
                        using (var stream=new MemoryStream()) { frame.Save(stream,ImageFormat.Png); frames.Add(stream.ToArray()); }
                        int shown=Math.Min(size,128);
                        preview.DrawImage(frame,px,12,shown,shown); preview.DrawImage(frame,px,162,shown,shown);
                        px+=shown+16;
                    }
                }
                sheet.Save(Path.Combine(output,"sizes-light-dark.png"),ImageFormat.Png);
            }
            using (var writer=new BinaryWriter(File.Create(Path.Combine(output,"rusty-note.ico")))) {
                writer.Write((ushort)0); writer.Write((ushort)1); writer.Write((ushort)sizes.Length);
                int offset=6+16*sizes.Length;
                for (int i=0; i<sizes.Length; i++) {
                    writer.Write((byte)(sizes[i]==256?0:sizes[i])); writer.Write((byte)(sizes[i]==256?0:sizes[i]));
                    writer.Write((byte)0); writer.Write((byte)0); writer.Write((ushort)1); writer.Write((ushort)32);
                    writer.Write(frames[i].Length); writer.Write(offset); offset+=frames[i].Length;
                }
                foreach (byte[] frame in frames) writer.Write(frame);
            }
        }
        using (var check=new Icon(Path.Combine(output,"rusty-note.ico"),32,32)) {
            if (check.Width!=32 || check.Height!=32) throw new Exception("ICO decoding self-check failed");
        }
        Console.WriteLine("Refined {0} paper pixels; verified eight transparent ICO sizes (16-256).",count);
    }
}
'@
[RustyIcon]::Build((Join-Path $root 'assets/icon-source.png'),$out)
Write-Output "Icon assets and size preview: $out"
